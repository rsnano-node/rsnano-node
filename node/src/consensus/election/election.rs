use std::{
    collections::HashMap,
    fmt::Debug,
    time::{Duration, SystemTime},
};

use strum_macros::{EnumCount, EnumIter};

use rsnano_nullable_clock::Timestamp;
use rsnano_types::{
    Account, Amount, Block, BlockHash, MaybeSavedBlock, PublicKey, QualifiedRoot, SavedBlock,
    UnixMillisTimestamp, Vote, VoteError,
};
use rsnano_utils::stats::DetailType;

use super::{ConfirmationType, ConfirmedElection, ElectionState, block_tallies::BlockTallies};

#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum VoteType {
    NonFinal,
    Final,
}

#[derive(Clone)]
pub struct Election {
    qualified_root: QualifiedRoot,
    winner: MaybeSavedBlock,
    state: ElectionState,
    // TODO: there can't be more than 10 blocks, so an array might be a lot faster
    candidate_blocks: HashMap<BlockHash, MaybeSavedBlock>,
    votes: HashMap<PublicKey, VoteSummary>,
    winner_tally: Amount,
    winner_final_tally: Amount,

    /// All tallies (non-final or final)
    tallies: BlockTallies,
    final_tallies: BlockTallies,

    behavior: ElectionBehavior,
    has_quorum: bool,

    start: Timestamp,
    /// Minimum time between broadcasts of the current winner of an election, as a backup to requesting confirmations
    base_latency: Duration,
    account: Account,
    last_final_vote: Option<Timestamp>,
    last_non_final_vote: Option<Timestamp>,
    last_voted_winner: BlockHash,
    #[cfg(feature = "ledger_snapshots")]
    has_voted: bool,
}

impl Election {
    const PASSIVE_DURATION_FACTOR: u32 = 5;
    pub const MAX_BLOCKS: usize = 10;

    pub fn new(
        block: SavedBlock,
        behavior: ElectionBehavior,
        base_latency: Duration,
        now: Timestamp,
    ) -> Self {
        Self {
            qualified_root: block.qualified_root(),
            votes: HashMap::new(),
            candidate_blocks: HashMap::from([(
                block.hash(),
                MaybeSavedBlock::Saved(block.clone()),
            )]),
            state: ElectionState::Passive,
            tallies: BlockTallies::new(),
            final_tallies: BlockTallies::new(),
            winner_tally: Amount::ZERO,
            winner_final_tally: Amount::ZERO,
            behavior,
            has_quorum: false,
            start: now,
            base_latency,
            account: block.account(),
            winner: MaybeSavedBlock::Saved(block),
            last_voted_winner: BlockHash::ZERO,
            last_non_final_vote: None,
            last_final_vote: None,
            #[cfg(feature = "ledger_snapshots")]
            has_voted: false,
        }
    }

    pub fn new_test_instance_with(block: SavedBlock) -> Self {
        Self::new(
            block,
            ElectionBehavior::Priority,
            Duration::from_millis(1000),
            Timestamp::new_test_instance(),
        )
    }

    pub fn qualified_root(&self) -> &QualifiedRoot {
        &self.qualified_root
    }

    pub fn behavior(&self) -> ElectionBehavior {
        self.behavior
    }

    pub fn account(&self) -> Account {
        self.account
    }

    pub fn state(&self) -> ElectionState {
        self.state
    }

    pub fn candidate_blocks(&self) -> &HashMap<BlockHash, MaybeSavedBlock> {
        &self.candidate_blocks
    }

    pub fn contains_block(&self, hash: &BlockHash) -> bool {
        self.candidate_blocks.contains_key(hash)
    }

    pub fn block_count(&self) -> usize {
        self.candidate_blocks.len()
    }

    pub fn has_max_blocks(&self) -> bool {
        self.block_count() >= Self::MAX_BLOCKS
    }

    pub fn try_add_fork(&mut self, fork: &Block, fork_tally: Amount) -> AddForkResult {
        // Do not insert new blocks if already confirmed
        if self.state.has_ended() {
            return AddForkResult::ElectionEnded;
        }

        if self.contains_block(&fork.hash()) {
            return AddForkResult::Duplicate;
        }

        let mut removed = None;
        if self.has_max_blocks() {
            removed = self.remove_tally_below(fork_tally);
            if removed.is_none() {
                return AddForkResult::TallyTooLow;
            }
        }

        self.tallies.insert(fork.hash(), fork_tally);
        self.candidate_blocks
            .insert(fork.hash(), MaybeSavedBlock::Unsaved(fork.clone()));

        match removed {
            Some(removed) => AddForkResult::Replaced(removed),
            None => AddForkResult::Added,
        }
    }

    pub fn votes(&self) -> &HashMap<PublicKey, VoteSummary> {
        &self.votes
    }

    pub fn add_vote(
        &mut self,
        voter: PublicKey,
        hash: BlockHash,
        vote_created: UnixMillisTimestamp,
        vote_received: Timestamp,
    ) {
        debug_assert!(self.candidate_blocks.contains_key(&hash));
        self.votes.insert(
            voter,
            VoteSummary::new(voter, hash, vote_created, vote_received),
        );
    }

    pub fn voted(&mut self, vote_type: VoteType, timestamp: Timestamp) {
        match vote_type {
            VoteType::NonFinal => self.last_non_final_vote = Some(timestamp),
            VoteType::Final => self.last_final_vote = Some(timestamp),
        }
        self.last_voted_winner = self.winner().hash();
        #[cfg(feature = "ledger_snapshots")]
        {
            self.has_voted = true;
        }
    }

    pub fn can_vote(&self, broadcast_interval: Duration, now: Timestamp) -> bool {
        #[cfg(feature = "ledger_snapshots")]
        if self.has_voted {
            return false;
        }

        if self.last_voted_winner != self.winner().hash() {
            return true;
        }

        let last_vote = match self.vote_type() {
            VoteType::NonFinal => self.last_non_final_vote,
            VoteType::Final => self.last_final_vote,
        };

        match last_vote {
            Some(last) => now >= last + broadcast_interval,
            None => true,
        }
    }

    pub fn winner_tally(&self) -> Amount {
        self.winner_tally
    }

    pub fn winner_final_tally(&self) -> Amount {
        self.winner_final_tally
    }

    /// Tallies for the candidate blocks, ordered by descending tally
    pub fn tallies(&self) -> &BlockTallies {
        &self.tallies
    }

    pub fn transition_time(&mut self, now: Timestamp) {
        let duration = self.start.elapsed(now);
        match self.state {
            ElectionState::Passive => {
                if self.base_latency * Self::PASSIVE_DURATION_FACTOR < duration {
                    self.state = ElectionState::Active;
                }
            }
            ElectionState::Confirmed => {
                self.state = ElectionState::ExpiredConfirmed;
            }
            _ => {}
        }

        if !self.state.has_ended() && self.behavior.time_to_live() < duration {
            self.state = ElectionState::ExpiredUnconfirmed;
        }
    }

    pub fn base_latency(&self) -> Duration {
        self.base_latency
    }

    pub fn has_quorum(&self) -> bool {
        self.has_quorum
    }

    /// Returns true if final votes should be generated
    pub fn is_final(&self) -> bool {
        self.is_confirmed() || self.has_quorum()
    }

    pub fn vote_type(&self) -> VoteType {
        if self.is_final() {
            VoteType::Final
        } else {
            VoteType::NonFinal
        }
    }

    pub fn cancel(&mut self) {
        if !self.state.has_ended() {
            self.state = ElectionState::Cancelled;
        }
    }

    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }

    pub fn transition_active(&mut self) {
        if self.state == ElectionState::Passive {
            self.state = ElectionState::Active;
        }
    }

    pub fn maybe_upgrade_to(&mut self, new_behavior: ElectionBehavior) -> bool {
        if new_behavior != ElectionBehavior::Priority {
            // Only upgrades to priority elections are allowed to enable immediate vote broadcasting!
            return false;
        }

        if matches!(
            self.behavior,
            ElectionBehavior::Priority | ElectionBehavior::Manual
        ) {
            // Nothing to do;
            return false;
        }

        self.behavior = ElectionBehavior::Priority;
        true
    }

    pub fn is_confirmed(&self) -> bool {
        self.state.is_confirmed()
    }

    pub fn winner(&self) -> &MaybeSavedBlock {
        &self.winner
    }

    pub fn force_confirm(&mut self) -> bool {
        if !self.state.has_ended() {
            self.state = ElectionState::Confirmed;
            true
        } else {
            false
        }
    }

    pub fn start(&self) -> Timestamp {
        self.start
    }

    pub fn remove_tally_below(&mut self, min_tally: Amount) -> Option<MaybeSavedBlock> {
        if min_tally.is_zero() {
            return None;
        }

        let mut block_to_remove = BlockHash::ZERO;
        let winner_hash = self.winner.hash();

        // Replace if lowest tally is below inactive cache new block weight
        if self.tallies.len() < Self::MAX_BLOCKS {
            // If count of tally items is less than 10, remove any block without tally
            for hash in self.candidate_blocks.keys() {
                if !self.tallies.contains(hash) && *hash != winner_hash {
                    block_to_remove = *hash;
                    break;
                }
            }
        }

        if block_to_remove.is_zero() {
            let (lowest_hash, lowest_tally) = self.tallies.lowest().unwrap();
            if min_tally > *lowest_tally {
                if *lowest_hash != winner_hash {
                    block_to_remove = *lowest_hash;
                } else {
                    // Avoid removing winner
                    let (second_lowest_hash, second_lowest_tally) =
                        self.tallies.iter().rev().nth(1).unwrap();

                    if min_tally > *second_lowest_tally {
                        block_to_remove = *second_lowest_hash;
                    }
                }
            }
        }

        if !block_to_remove.is_zero() {
            self.remove_block(&block_to_remove)
        } else {
            None
        }
    }

    /// Calculate tallies and try to confirm this election
    pub fn update_tallies(
        &mut self,
        rep_weights: &HashMap<PublicKey, Amount>,
        quorum_delta: Amount,
    ) {
        if self.state.has_ended() {
            return;
        }

        self.update_vote_weights(rep_weights);
        self.recalculate_tallies();

        if let Some(new_winner) = self.check_new_winner(quorum_delta) {
            tracing::warn!("Winner changed to {:?}!", new_winner);
            self.change_winner_to(&new_winner);
        }

        self.update_winner_tally();
        self.try_set_quorum(quorum_delta);
        self.try_confirm(quorum_delta);
    }

    fn update_vote_weights(&mut self, rep_weights: &HashMap<PublicKey, Amount>) {
        for vote in self.votes.values_mut() {
            vote.weight = rep_weights.get(&vote.voter).cloned().unwrap_or_default();
        }
    }

    fn recalculate_tallies(&mut self) {
        self.tallies.calculate(self.votes.values());
        self.final_tallies
            .calculate(self.votes.values().filter(|v| v.is_final_vote()));
    }

    fn check_new_winner(&self, quorum_delta: Amount) -> Option<BlockHash> {
        if self.tallies.sum() < quorum_delta {
            // The winner can only be changed after a super majority of votes has been observed!
            return None;
        }

        let old_winner = self.winner.hash();
        let new_winner = self.tallies.winner().map(|(h, _)| *h).unwrap_or(old_winner);
        if new_winner != old_winner {
            Some(new_winner)
        } else {
            None
        }
    }

    fn change_winner_to(&mut self, new_winner: &BlockHash) {
        self.winner = self.candidate_blocks().get(new_winner).unwrap().clone();
    }

    fn update_winner_tally(&mut self) {
        let winner_hash = self.winner.hash();
        self.winner_tally = self.tallies.get(&winner_hash);
        self.winner_final_tally = self.final_tallies.get(&winner_hash);
    }

    fn try_set_quorum(&mut self, quorum_delta: Amount) {
        if self.tallies.check_quorum(quorum_delta) {
            self.has_quorum = true;
        }
    }

    fn try_confirm(&mut self, quorum_delta: Amount) {
        if self.winner_final_tally >= quorum_delta {
            self.state = ElectionState::Confirmed;
        }
    }

    pub fn remove_vote(&mut self, voter: &PublicKey) {
        self.votes.remove(voter);
    }

    fn remove_block(&mut self, hash: &BlockHash) -> Option<MaybeSavedBlock> {
        if self.winner.hash() != *hash {
            let existing = self.candidate_blocks.remove(hash);
            if existing.is_some() {
                self.votes.retain(|_, v| v.hash != *hash);
                self.tallies.remove(hash);
                self.final_tallies.remove(hash);
                return existing;
            }
        }

        None
    }

    /// TODO: Remove as soon as possible
    pub fn change_received_timestamp(&mut self, voter: &PublicKey, new_timestamp: Timestamp) {
        self.votes.get_mut(voter).unwrap().vote_received = new_timestamp;
    }

    pub fn into_confirmed_election(
        &self,
        now: Timestamp,
        result: ConfirmationType,
    ) -> ConfirmedElection {
        let votes = self.votes().clone();

        ConfirmedElection {
            winner: self.winner().clone(),
            tally: self.winner_tally(),
            final_tally: self.winner_final_tally(),
            block_count: self.block_count() as u32,
            voter_count: self.votes().len() as u32,
            election_duration: self.start().elapsed(now),
            election_end: SystemTime::now(),
            confirmation_type: result,
            votes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoteSummary {
    pub voter: PublicKey,
    pub vote_created: UnixMillisTimestamp,
    pub vote_received: Timestamp, // TODO use Instant
    pub hash: BlockHash,
    pub weight: Amount,
}

impl VoteSummary {
    pub fn new(
        voter: PublicKey,
        hash: BlockHash,
        vote_created: UnixMillisTimestamp,
        vote_received: Timestamp,
    ) -> Self {
        Self {
            voter,
            vote_received,
            vote_created,
            hash,
            weight: Amount::ZERO,
        }
    }

    pub fn is_final_vote(&self) -> bool {
        self.vote_created == UnixMillisTimestamp::MAX
    }

    pub fn ensure_no_replay(
        &self,
        new_vote: &Vote,
        block_hash: &BlockHash,
    ) -> Result<(), VoteError> {
        if self.vote_created > new_vote.timestamp() {
            Err(VoteError::Replay)
        } else if self.vote_created == new_vote.timestamp() && self.hash >= *block_hash {
            Err(VoteError::Replay)
        } else {
            Ok(())
        }
    }

    pub fn has_switched_to_final_vote(&self, new_vote: &Vote) -> bool {
        new_vote.is_final() && self.vote_created < new_vote.timestamp()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumIter)]
pub enum ElectionBehavior {
    Manual,
    Priority,
    /**
     * Hinted elections:
     * - shorter timespan
     * - limited space inside AEC
     */
    Hinted,
    /**
     * Optimistic elections:
     * - shorter timespan
     * - limited space inside AEC
     * - more frequent confirmation requests
     */
    Optimistic,
}

impl ElectionBehavior {
    fn time_to_live(&self) -> Duration {
        match self {
            ElectionBehavior::Manual | ElectionBehavior::Priority => Duration::from_secs(60 * 5),
            ElectionBehavior::Hinted | ElectionBehavior::Optimistic => Duration::from_secs(30),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ElectionBehavior::Manual => "manual",
            ElectionBehavior::Priority => "priority",
            ElectionBehavior::Hinted => "hinted",
            ElectionBehavior::Optimistic => "optimistic",
        }
    }
}

impl From<ElectionBehavior> for DetailType {
    fn from(value: ElectionBehavior) -> Self {
        match value {
            ElectionBehavior::Manual => DetailType::Manual,
            ElectionBehavior::Priority => DetailType::Priority,
            ElectionBehavior::Hinted => DetailType::Hinted,
            ElectionBehavior::Optimistic => DetailType::Optimistic,
        }
    }
}

pub enum AddForkResult {
    Added,
    Replaced(MaybeSavedBlock),
    TallyTooLow,
    Duplicate,
    ElectionEnded,
}
