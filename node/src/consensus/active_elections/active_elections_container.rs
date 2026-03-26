use std::time::Duration;

use strum::EnumCount;

use rsnano_ledger::RepWeights;
use rsnano_nullable_clock::Timestamp;
use rsnano_types::{Amount, Block, BlockHash, PublicKey, QualifiedRoot, SavedBlock};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{StatsCollection, StatsSource},
};

use crate::{
    consensus::{
        election::{
            AddForkResult, ConfirmationType, ConfirmedElection, Election, ElectionBehavior,
            VoteType,
        },
        election_schedulers::priority::PriorityBucketState,
        filtered_vote::FilteredVote,
    },
    representatives::QuorumSpecs,
};

use super::{
    ActiveElectionsConfig, ActiveElectionsInfo, AecFact, AecFacts, AecInsertError,
    AecInsertRequest, Entry, RootContainer,
    apply_vote_helper::{ApplyVoteHelper, ApplyVoteResult},
    cooldown_controller::{AecCooldownReason, CooldownController, CooldownResult},
    recently_confirmed_cache::RecentlyConfirmedCache,
    stats::AecStats,
};

pub struct ActiveElectionsContainer {
    roots: RootContainer,
    stopped: bool,
    count_by_behavior: [usize; ElectionBehavior::COUNT],
    base_latency: Duration,
    recently_confirmed: RecentlyConfirmedCache,
    cooldown: CooldownController,
    max_elections: usize,
    stats: AecStats,
}

impl ActiveElectionsContainer {
    pub fn new(config: ActiveElectionsConfig, base_latency: Duration) -> Self {
        Self {
            roots: RootContainer::default(),
            stopped: false,
            count_by_behavior: Default::default(),
            base_latency,
            recently_confirmed: RecentlyConfirmedCache::new(config.confirmation_cache),
            cooldown: CooldownController::default(),
            max_elections: config.max_elections,
            stats: Default::default(),
        }
    }

    pub fn max_len(&self) -> usize {
        self.max_elections
    }

    pub fn count_by_behavior(&self, behavior: ElectionBehavior) -> usize {
        self.count_by_behavior[behavior as usize]
    }

    fn count_by_behavior_mut(&mut self, behavior: ElectionBehavior) -> &mut usize {
        &mut self.count_by_behavior[behavior as usize]
    }

    pub(crate) fn priority_bucket_state(
        &self,
        bucket_id: usize,
        candidate_root: &QualifiedRoot,
    ) -> PriorityBucketState {
        PriorityBucketState {
            active_len: self.roots.bucket_len(bucket_id),
            contains_candidate: self.is_active_root(candidate_root),
            lowest: self.roots.lowest_priority(bucket_id),
            is_cooling_down: self.cooldown.is_cooling_down(),
            vacancy: self.vacancy(),
        }
    }

    pub fn iter_round_robin(&self) -> impl Iterator<Item = &Election> {
        self.roots.iter().map(|i| &i.election)
    }

    pub fn iter_bucket(&self, bucket_id: usize) -> impl Iterator<Item = &Election> {
        self.roots.iter_bucket(bucket_id).map(|i| &i.election)
    }

    pub(crate) fn insert(
        &mut self,
        request: AecInsertRequest,
        now: Timestamp,
    ) -> Result<AecFacts, AecInsertError> {
        self.ensure_not_stopped()?;
        self.ensure_not_recently_confirmed(&request)?;

        if self.try_upgrade_priority_election(&request)? {
            return Ok(AecFacts::new());
        }

        Ok(self.insert_new_election(request, now))
    }

    pub fn set_last_voted(
        &mut self,
        root: &QualifiedRoot,
        vote_type: VoteType,
        timestamp: Timestamp,
    ) {
        let Some(entry) = self.roots.get_mut(root) else {
            return;
        };
        entry.election.voted(vote_type, timestamp);
    }

    fn ensure_not_stopped(&self) -> Result<(), AecInsertError> {
        if self.stopped {
            Err(AecInsertError::Stopped)
        } else {
            Ok(())
        }
    }

    fn ensure_not_recently_confirmed(
        &self,
        request: &AecInsertRequest,
    ) -> Result<(), AecInsertError> {
        let root = request.block.qualified_root();

        if self.recently_confirmed.root_exists(&root) {
            return Err(AecInsertError::RecentlyConfirmed);
        }
        Ok(())
    }

    fn try_upgrade_priority_election(
        &mut self,
        request: &AecInsertRequest,
    ) -> Result<bool, AecInsertError> {
        let (upgraded, previous_behavior) = self.roots.try_upgrade_to_priority_election(request);

        if upgraded {
            *self.count_by_behavior_mut(previous_behavior.unwrap()) -= 1;
            *self.count_by_behavior_mut(request.behavior) += 1;
            Ok(true)
        } else if previous_behavior.is_some() {
            Err(AecInsertError::Duplicate)
        } else {
            Ok(false)
        }
    }

    fn insert_new_election(&mut self, request: AecInsertRequest, now: Timestamp) -> AecFacts {
        let root = request.block.qualified_root();
        let hash = request.block.hash();
        let election = Election::new(request.block, request.behavior, self.base_latency, now);

        self.roots.insert(Entry {
            root: root.clone(),
            election,
            priority: request.priority,
        });

        *self.count_by_behavior_mut(request.behavior) += 1;
        self.stats.started(request.behavior);
        AecFact::ElectionStarted(hash, root).into()
    }

    pub(crate) fn try_add_fork(&mut self, fork: &Block, fork_tally: Amount) -> (bool, AecFacts) {
        let Some(entry) = self.roots.get_mut(&fork.qualified_root()) else {
            return (false, AecFacts::new());
        };

        let result = entry.election.try_add_fork(fork, fork_tally);
        let mut facts = AecFacts::new();
        let added = match result {
            AddForkResult::Added => {
                facts.push(AecFact::BlockAddedToElection(fork.hash()));
                true
            }
            AddForkResult::Replaced(removed) => {
                self.roots.vote_router.disconnect(&removed.hash());
                facts.push(AecFact::BlockDiscarded(removed.into()));
                facts.push(AecFact::BlockAddedToElection(fork.hash()));
                true
            }
            AddForkResult::TallyTooLow => {
                facts.push(AecFact::BlockDiscarded(fork.clone()));
                false
            }
            AddForkResult::Duplicate | AddForkResult::ElectionEnded => false,
        };

        if added {
            self.roots
                .vote_router
                .connect(fork.hash(), fork.qualified_root());
            self.stats.conflicts += 1;
        }

        (added, facts)
    }

    /// How many election slots are available
    /// This is a soft limit and can be negative!
    pub fn vacancy(&self) -> i64 {
        if self.cooldown.is_cooling_down() {
            return 0;
        }
        let current_size = self.roots.len() as i64;
        self.max_elections as i64 - current_size
    }

    pub(crate) fn set_cooldown(&mut self, cool_down: bool, reason: AecCooldownReason) -> AecFacts {
        let result = self.cooldown.set_cooldown(cool_down, reason);
        if result == CooldownResult::Recovered {
            AecFact::Recovered.into()
        } else {
            AecFacts::new()
        }
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        self.roots.clear();
    }

    pub fn is_active_root(&self, root: &QualifiedRoot) -> bool {
        self.roots.get(root).is_some()
    }

    pub fn is_active_hash(&self, block_hash: &BlockHash) -> bool {
        self.roots.vote_router.is_active(block_hash)
    }

    pub fn was_recently_confirmed(&self, block_hash: &BlockHash) -> bool {
        self.recently_confirmed.hash_exists(block_hash)
    }

    pub fn clear_recently_confirmed(&mut self) {
        self.recently_confirmed.clear();
    }

    /// Returns the current active elections after transitioning
    pub(crate) fn transition_time(&mut self, now: Timestamp) -> AecFacts {
        self.stats.ticked += 1;
        for entry in self.roots.iter_mut() {
            entry.election.transition_time(now);
        }
        self.erase_ended_elections()
    }

    pub fn election_for_root(&self, root: &QualifiedRoot) -> Option<&Election> {
        self.roots.election_for_root(root)
    }

    pub fn election_for_block(&self, block_hash: &BlockHash) -> Option<&Election> {
        self.roots.election_for_block(block_hash)
    }

    pub fn transition_active(&mut self, block_hash: &BlockHash) -> bool {
        let Some(election) = self.roots.election_for_block_mut(block_hash) else {
            return false;
        };
        election.transition_active();
        true
    }

    pub fn remove_votes<'a>(
        &mut self,
        root: &QualifiedRoot,
        voters: impl IntoIterator<Item = &'a PublicKey>,
    ) {
        let Some(election) = self.roots.election_for_root_mut(root) else {
            return;
        };
        for voter in voters {
            election.remove_vote(voter);
        }
    }

    pub(crate) fn erase_ended_elections(&mut self) -> AecFacts {
        let removed = self.roots.drain_filter(|i| i.election.state().has_ended());
        let mut facts = AecFacts::new();

        for entry in removed {
            facts.push(self.cleanup_election(entry));
        }
        facts
    }

    pub(crate) fn erase(&mut self, root: &QualifiedRoot) -> Option<AecFacts> {
        let Some(entry) = self.roots.erase(root) else {
            return None;
        };
        Some(self.cleanup_election(entry).into())
    }

    pub(crate) fn replace_lowest_priority(
        &mut self,
        root: &QualifiedRoot,
        request: AecInsertRequest,
        now: Timestamp,
    ) -> Result<AecFacts, AecInsertError> {
        self.ensure_not_stopped()?;
        self.ensure_not_recently_confirmed(&request)?;

        let Some(erased) = self.roots.erase(root) else {
            return Err(AecInsertError::Duplicate);
        };

        let mut facts = AecFacts::from(self.cleanup_election(erased));
        facts.extend(self.insert_new_election(request, now));
        Ok(facts)
    }

    fn cleanup_election(&mut self, entry: Entry) -> AecFact {
        let election = &entry.election;

        // Keep track of election count by election type
        *self.count_by_behavior_mut(election.behavior()) -= 1;

        self.stats.stopped(&entry.election);
        AecFact::ElectionEnded(entry.election)
    }

    /// Dependent elections are implicitly confirmed when their block is confirmed
    pub(crate) fn confirm_dependent_elections(
        &mut self,
        confirmed: Vec<(SavedBlock, Option<ConfirmedElection>)>,
        now: Timestamp,
    ) -> AecFacts {
        let mut facts = AecFacts::new();
        for (confirmed_block, source_election) in confirmed {
            let confirmed_election =
                self.confirm_dependent_election(&confirmed_block, source_election, now);

            facts.push(self.block_confirmed(confirmed_block, confirmed_election));
        }
        facts
    }

    fn confirm_dependent_election(
        &mut self,
        confirmed_block: &SavedBlock,
        source_election: Option<ConfirmedElection>,
        now: Timestamp,
    ) -> ConfirmedElection {
        // Check if the currently confirmed block was part of an election that triggered
        // the block confirmation
        if let Some(source) = source_election
            && confirmed_block.hash() == source.winner.hash()
        {
            // This is the block that was directly confirmed by the source election.
            // The election is already confirmed, so there is nothing to do.
            return source;
        }

        let Some(corresponding) = self.roots.get_mut(&confirmed_block.qualified_root()) else {
            return ConfirmedElection::new(
                confirmed_block.clone(),
                ConfirmationType::InactiveConfirmationHeight,
            );
        };

        if corresponding.election.winner().hash() == confirmed_block.hash() {
            corresponding.election.force_confirm();
            corresponding
                .election
                .into_confirmed_election(now, ConfirmationType::ActiveConfirmationHeight)
        } else {
            corresponding.election.cancel();
            ConfirmedElection::new(
                confirmed_block.clone(),
                ConfirmationType::ActiveConfirmationHeight,
            )
        }
    }

    fn block_confirmed(&mut self, block: SavedBlock, election: ConfirmedElection) -> AecFact {
        self.stats.block_confirmations[election.confirmation_type as usize] += 1;
        AecFact::BlockConfirmed(block, election)
    }

    pub fn remove_recently_confirmed(&mut self, block_hash: &BlockHash) {
        self.recently_confirmed.erase(block_hash);
    }

    pub(crate) fn apply_vote<'a>(&mut self, args: ApplyVoteArgs<'a>) -> ApplyVoteResult {
        let mut apply_helper = ApplyVoteHelper {
            args: &args,
            recently_confirmed: &mut self.recently_confirmed,
            vote_counter: &mut self.stats.vote_counter,
            roots: &mut self.roots,
        };
        let mut result = apply_helper.apply_vote();
        for entry in std::mem::take(&mut result.confirmed) {
            result.facts.push(self.cleanup_election(entry));
        }
        result
    }

    pub(crate) fn force_confirm(&mut self, block_hash: &BlockHash, now: Timestamp) -> AecFacts {
        let Some(election) = self.roots.election_for_block_mut(block_hash) else {
            panic!("Force confirm failed, because no active election was found");
        };
        if election.force_confirm() {
            let confirmed_election =
                election.into_confirmed_election(now, ConfirmationType::ActiveConfirmedQuorum);
            AecFact::ElectionConfirmed(confirmed_election).into()
        } else {
            AecFacts::new()
        }
    }

    pub fn cancel(&mut self, root: &QualifiedRoot) {
        if let Some(entry) = self.roots.get_mut(root) {
            entry.election.cancel();
        }
    }

    pub fn cancel_all(&mut self) {
        for entry in self.roots.iter_mut() {
            entry.election.cancel();
        }
    }

    pub fn len(&self) -> usize {
        self.roots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn info(&self) -> ActiveElectionsInfo {
        ActiveElectionsInfo {
            max_elections: self.max_elections,
            total: self.roots.len(),
            priority: self.count_by_behavior(ElectionBehavior::Priority),
            hinted: self.count_by_behavior(ElectionBehavior::Hinted),
            optimistic: self.count_by_behavior(ElectionBehavior::Optimistic),
        }
    }
}

impl Default for ActiveElectionsContainer {
    fn default() -> Self {
        Self::new(ActiveElectionsConfig::default(), Duration::from_secs(1))
    }
}

impl StatsSource for ActiveElectionsContainer {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.cooldown.collect_stats(result);
        self.stats.collect_stats(result);
    }
}

impl ContainerInfoProvider for ActiveElectionsContainer {
    fn container_info(&self) -> ContainerInfo {
        ContainerInfo::builder()
            .leaf("roots", self.roots.len(), RootContainer::ELEMENT_SIZE)
            .leaf(
                "normal",
                self.count_by_behavior(ElectionBehavior::Priority),
                0,
            )
            .leaf(
                "hinted".to_string(),
                self.count_by_behavior(ElectionBehavior::Hinted),
                0,
            )
            .leaf(
                "optimistic".to_string(),
                self.count_by_behavior(ElectionBehavior::Optimistic),
                0,
            )
            .node(
                "recently_confirmed",
                self.recently_confirmed.container_info(),
            )
            .node("vote_router", self.roots.vote_router.container_info())
            .finish()
    }
}

pub struct ApplyVoteArgs<'a> {
    pub vote: &'a FilteredVote,
    pub rep_weights: &'a RepWeights,
    pub quorum_specs: &'a QuorumSpecs,
    pub now: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::ReceivedVote;
    use rsnano_types::{BlockPriority, PrivateKey, TimePriority, Vote, VoteSource};
    use std::sync::Arc;

    #[test]
    fn empty() {
        let container = ActiveElectionsContainer::default();
        assert_eq!(container.len(), 0);
        assert!(!container.is_active_root(&QualifiedRoot::new_test_instance()));
        assert!(!container.is_active_hash(&BlockHash::from(1)));
    }

    #[test]
    fn insert_election() {
        let mut container = ActiveElectionsContainer::default();
        let request = AecInsertRequest {
            block: SavedBlock::new_test_instance(),
            behavior: ElectionBehavior::Priority,
            priority: BlockPriority::new_test_instance(),
        };

        let facts = container
            .insert(request, Timestamp::new_test_instance())
            .unwrap();

        assert_eq!(container.len(), 1);
        assert!(matches!(facts.as_slice(), [AecFact::ElectionStarted(_, _)]));
    }

    #[test]
    fn confirm_election() {
        let mut container = ActiveElectionsContainer::default();

        let block = SavedBlock::new_test_instance();
        let block_hash = block.hash();

        let request = AecInsertRequest {
            block,
            behavior: ElectionBehavior::Priority,
            priority: BlockPriority::new_test_instance(),
        };

        let now = Timestamp::new_test_instance();
        let insert_facts = container.insert(request, now).unwrap();

        let rep_key = PrivateKey::from(1);
        let received_vote = test_final_vote(&rep_key, block_hash);

        let mut rep_weights = RepWeights::default();
        rep_weights.put(rep_key.public_key(), Amount::MAX);

        let result = container.apply_vote(ApplyVoteArgs {
            vote: &received_vote.into(),
            rep_weights: &rep_weights,
            quorum_specs: &QuorumSpecs::new_test_instance(),
            now,
        });

        assert_eq!(result.per_block.get(&block_hash), Some(&Ok(())));
        assert!(matches!(
            insert_facts.as_slice(),
            [AecFact::ElectionStarted(_, _)]
        ));
        assert!(matches!(
            result.facts.as_slice(),
            [AecFact::ElectionConfirmed(_), AecFact::ElectionEnded(_)]
        ));

        assert!(container.election_for_block(&block_hash).is_none());
    }

    #[test]
    fn iter_round_robin() {
        let block_a = SavedBlock::new_test_instance_with_key(1);
        let block_b = SavedBlock::new_test_instance_with_key(2);
        let block_c = SavedBlock::new_test_instance_with_key(3);
        let block_d = SavedBlock::new_test_instance_with_key(4);

        let prio_a = BlockPriority::new(Amount::nano(1), TimePriority::new(100));
        let prio_b = BlockPriority::new(Amount::nano(100), TimePriority::new(100));
        let prio_c = BlockPriority::new(Amount::nano(100), TimePriority::new(99));
        let prio_d = BlockPriority::new(Amount::nano(1_000_000), TimePriority::new(100));

        test_iter(&[], &[]);

        test_iter(&[(&block_a, prio_a)], &[&block_a]);

        test_iter(
            &[
                (&block_d, prio_d),
                (&block_a, prio_a),
                (&block_c, prio_c),
                (&block_b, prio_b),
            ],
            &[&block_d, &block_c, &block_a, &block_b],
        )
    }

    fn test_final_vote(rep_key: &PrivateKey, block_hash: BlockHash) -> ReceivedVote {
        let vote = Arc::new(Vote::new_final(rep_key, vec![block_hash]));
        ReceivedVote::new(vote, VoteSource::Live, None)
    }

    fn test_iter(blocks: &[(&SavedBlock, BlockPriority)], expected: &[&SavedBlock]) {
        let mut container = ActiveElectionsContainer::default();

        for (block, prio) in blocks {
            let request = AecInsertRequest::new_priority((**block).clone(), *prio);

            container
                .insert(request, Timestamp::new_test_instance())
                .unwrap();
        }

        let result: Vec<_> = container
            .iter_round_robin()
            .map(|i| i.winner().hash())
            .collect();
        let expected: Vec<_> = expected.iter().map(|i| i.hash()).collect();
        assert_eq!(result, expected);
    }
}
