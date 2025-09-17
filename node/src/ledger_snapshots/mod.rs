mod tally;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rsnano_ledger::Ledger;
use rsnano_messages::{Aggregatable, Message, Preproposal, Proposal, ProposalHash, ProposalVote};
use rsnano_network::TrafficType;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Amount, PrivateKey};
use rsnano_types::{Account, BlockHash};

use crate::ledger_snapshots::tally::Aggregator;
use crate::representatives::{ConsensusParams, OnlineReps};
use crate::transport::MessageFlooder;

pub struct LedgerSnapshots {
    ledger: Arc<Ledger>,
    /// For simplicity we currently assume that there is at most
    /// one representative key!
    /// TODO: We have to extend this later to multiple representatives per node.
    get_private_key: Box<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
    flooder: Mutex<MessageFlooder>,
    receive_preproposal_listener: OutputListenerMt<Preproposal>,
    receive_proposal_listener: OutputListenerMt<Proposal>,
    receive_proposal_vote_listener: OutputListenerMt<ProposalVote>,
    preproposal_aggregator: Mutex<Aggregator<Preproposal>>,
    proposal_aggregator: Mutex<Aggregator<Proposal>>,
    proposal_vote_aggregator: Mutex<Aggregator<ProposalVote>>,
    online_reps: Arc<Mutex<OnlineReps>>,
    proposal_voted: AtomicBool,
}

impl LedgerSnapshots {
    pub fn new(
        ledger: Arc<Ledger>,
        get_private_key: impl Fn() -> Option<PrivateKey> + Send + Sync + 'static,
        flooder: MessageFlooder,
        online_reps: Arc<Mutex<OnlineReps>>,
    ) -> Self {
        Self {
            ledger,
            get_private_key: Box::new(get_private_key),
            flooder: flooder.into(),
            receive_preproposal_listener: OutputListenerMt::new(),
            receive_proposal_listener: OutputListenerMt::new(),
            receive_proposal_vote_listener: OutputListenerMt::new(),
            preproposal_aggregator: Default::default(),
            proposal_aggregator: Default::default(),
            proposal_vote_aggregator: Default::default(),
            online_reps,
            proposal_voted: AtomicBool::new(false),
        }
    }

    pub fn new_null() -> Self {
        Self::new(
            Ledger::new_null().into(),
            || None,
            MessageFlooder::new_null(),
            Mutex::new(OnlineReps::default()).into(),
        )
    }

    pub fn publish_preproposal(&self) {
        // TODO add test for no private key
        let private_key = (self.get_private_key)().unwrap();
        let preproposal = self.create_preproposal(&private_key);
        let message = Message::SnapshotPreproposal(preproposal);
        self.flooder.lock().unwrap().flood_prs_and_some_non_prs(
            &message,
            TrafficType::LedgerSnapshots,
            0.0,
        );
    }

    fn create_preproposal(&self, private_key: &PrivateKey) -> Preproposal {
        let frontiers = self.collect_frontiers();
        Preproposal::new(frontiers, private_key, 0)
    }

    fn collect_frontiers(&self) -> Vec<(Account, BlockHash)> {
        self.ledger.confirmed().frontiers().collect()
    }

    pub fn receive_preproposal(&self, preproposal: Preproposal) {
        self.receive_preproposal_listener.emit(preproposal.clone());

        let proposal = {
            let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

            let mut preproposal_aggregator = self.preproposal_aggregator.lock().unwrap();
            preproposal_aggregator.add(preproposal);

            if preproposal_aggregator.has_quorum(&consensus_params) {
                let proposal = Proposal::new(
                    preproposal_aggregator.values(),
                    &(self.get_private_key)().unwrap(),
                );
                Some(proposal)
            } else {
                None
            }
        };

        if let Some(proposal) = proposal {
            self.flooder.lock().unwrap().flood_prs_and_some_non_prs(
                &Message::SnapshotProposal(proposal),
                TrafficType::LedgerSnapshots,
                0.0,
            );
        };
    }

    pub fn track_received_preproposals(&self) -> Arc<OutputTrackerMt<Preproposal>> {
        self.receive_preproposal_listener.track()
    }

    pub fn receive_proposal(&self, proposal: Proposal) {
        self.receive_proposal_listener.emit(proposal.clone());

        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

        let mut proposal_aggregator = self.proposal_aggregator.lock().unwrap();
        proposal_aggregator.add(proposal);

        if proposal_aggregator.has_quorum(&consensus_params)
            && !self.proposal_voted.load(Ordering::SeqCst)
        {
            if let Some(proposal_vote) = LedgerSnapshots::create_proposal_vote(
                &proposal_aggregator,
                &(self.get_private_key)().unwrap(),
            ) {
                self.flooder.lock().unwrap().flood_prs_and_some_non_prs(
                    &Message::SnapshotProposalVote(proposal_vote),
                    TrafficType::LedgerSnapshots,
                    0.0,
                );
                self.proposal_voted.store(true, Ordering::SeqCst);
            }
        }
    }

    fn create_proposal_vote(
        proposal_aggregator: &Aggregator<Proposal>,
        private_key: &PrivateKey,
    ) -> Option<ProposalVote> {
        Some(ProposalVote::new(
            proposal_aggregator.values().map(|p| p.hash()).max()?,
            private_key,
        ))
    }

    pub fn track_received_proposals(&self) -> Arc<OutputTrackerMt<Proposal>> {
        self.receive_proposal_listener.track()
    }

    pub fn track_received_proposal_votes(&self) -> Arc<OutputTrackerMt<ProposalVote>> {
        self.receive_proposal_vote_listener.track()
    }

    pub fn receive_proposal_vote(&self, proposal_vote: ProposalVote) {
        self.receive_proposal_vote_listener
            .emit(proposal_vote.clone());

        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

        let mut vote_aggregator = self.proposal_vote_aggregator.lock().unwrap();
        vote_aggregator.add(proposal_vote);

        if let Some(winner) = LedgerSnapshots::find_winner_proposal(&consensus_params, vote_aggregator.values()) {
            tracing::warn!(proposal_hash=?winner, "Found a winner!");
        }
    }

    pub(crate) fn find_winner_proposal<'a>(
        params: &ConsensusParams,
        votes: impl IntoIterator<Item = &'a ProposalVote>,
    ) -> Option<ProposalHash> {
        let mut tallies: HashMap<ProposalHash, Amount> = HashMap::new();
    
        for vote in votes {
            let weight = tallies.entry(vote.proposal_hash).or_default();
            *weight += params.rep_weights.weight(&vote.voter);
        }
    
        tallies
            .into_iter()
            .find(|(_, w)| *w >= params.quorum_weight)
            .map(|(p, _)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{representatives::ONLINE_WEIGHT_QUORUM, transport::FloodEvent};
    use rsnano_ledger::RepWeights;
    use rsnano_messages::{Aggregatable, Message, ProposalVote};
    use rsnano_network::TrafficType;
    use rsnano_output_tracker::OutputTrackerMt;
    use rsnano_types::{AccountInfo, Amount, ConfirmationHeightInfo};
    use std::time::Duration;

    #[test]
    fn ledger_with_one_account() {
        let account = Account::from(1);
        let frontier = BlockHash::from(2);
        let fixture = Fixture::with_frontiers([(account, frontier)]);
        assert_eq!(fixture.snapshots.collect_frontiers(), [(account, frontier)]);
    }

    #[test]
    fn ledger_with_multiple_accounts() {
        let account1 = Account::from(1);
        let frontier1 = BlockHash::from(100);
        let account2 = Account::from(2);
        let frontier2 = BlockHash::from(200);

        let fixture = Fixture::with_frontiers([(account1, frontier1), (account2, frontier2)]);
        assert_eq!(
            fixture.snapshots.collect_frontiers(),
            [(account1, frontier1), (account2, frontier2)]
        );
    }

    #[test]
    fn create_preproposal() {
        let account = Account::from(10);
        let frontier = BlockHash::from(2);
        let fixture = Fixture::with_frontiers([(account, frontier)]);

        let preproposal = fixture.snapshots.create_preproposal(&PrivateKey::from(1));

        assert!(preproposal.frontiers.contains(&(account, frontier)));
    }

    #[test]
    fn publish_preproposal() {
        let account = Account::from(1);
        let frontier = BlockHash::from(100);
        let fixture = Fixture::with_frontiers([(account, frontier)]);

        fixture.snapshots.publish_preproposal();

        let flood_events = fixture.flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let expected_preproposal = fixture
            .snapshots
            .create_preproposal(&get_test_key().unwrap());

        assert_eq!(
            flood_events[0],
            FloodEvent {
                message: Message::SnapshotPreproposal(expected_preproposal),
                // TODO: add new traffic type for snapshots
                traffic_type: TrafficType::LedgerSnapshots,
                scale: 0.0,
                all_prs: true,
            }
        );
    }

    #[test]
    fn can_track_received_preproposals() {
        let fixture = Fixture::new();
        let preproposal = Preproposal::new_test_instance();
        fixture.snapshots.receive_preproposal(preproposal.clone());

        let receive_events = fixture.receive_preproposal_tracker.output();
        assert_eq!(receive_events.len(), 1, "Should receive preproposal");
        assert_eq!(receive_events[0], preproposal);
    }

    #[test]
    fn a_received_preproposal_is_added_to_the_preproposal_aggregator() {
        let fixture = Fixture::new();
        let snapshots = &fixture.snapshots;
        let preproposal = Preproposal::new_test_instance();

        snapshots.receive_preproposal(preproposal.clone());

        assert!(
            snapshots
                .preproposal_aggregator
                .lock()
                .unwrap()
                .contains(&preproposal.hash())
        );
    }

    #[test]
    fn publish_proposal_vote_when_quorum_of_preproposals_is_reached() {
        let mut rep_weights = RepWeights::new();
        let private_key = get_test_key().unwrap();
        let quorum_weight = Amount::nano(100_000);

        rep_weights.insert(private_key.public_key(), quorum_weight);
        let fixture = Fixture::with_rep_weights(rep_weights, quorum_weight);

        let preproposal = Preproposal::new(vec![], &private_key, 0);
        fixture.snapshots.receive_preproposal(preproposal.clone());

        let flood_events = fixture.flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let expected_proposal = Proposal::new(&[preproposal], &private_key);

        assert_eq!(
            flood_events[0],
            FloodEvent {
                message: Message::SnapshotProposal(expected_proposal),
                traffic_type: TrafficType::LedgerSnapshots,
                scale: 0.0,
                all_prs: true,
            }
        );
    }

    #[test]
    fn can_track_received_proposals() {
        let fixture = Fixture::new();
        let proposal = Proposal::new_test_instance();
        fixture.snapshots.receive_proposal(proposal.clone());

        let receive_events = fixture.receive_proposal_tracker.output();
        assert_eq!(receive_events.len(), 1, "Should receive proposal");
        assert_eq!(receive_events[0], proposal);
    }

    #[test]
    fn a_received_proposal_is_added_to_the_proposal_aggregator() {
        let fixture = Fixture::new();
        let snapshots = &fixture.snapshots;
        let proposal = Proposal::new_test_instance();

        snapshots.receive_proposal(proposal.clone());

        assert!(
            snapshots
                .proposal_aggregator
                .lock()
                .unwrap()
                .contains(&proposal.hash())
        );
    }

    #[test]
    fn publish_proposal_vote_when_quorum_of_proposals_is_reached() {
        let mut rep_weights = RepWeights::new();
        let private_key = get_test_key().unwrap();
        let quorum_weight = Amount::nano(100_000);

        rep_weights.insert(private_key.public_key(), quorum_weight);
        let fixture = Fixture::with_rep_weights(rep_weights, quorum_weight);

        let proposal = Proposal::new(vec![], &private_key);
        fixture.snapshots.receive_proposal(proposal.clone());

        let flood_events = fixture.flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let expected_proposal_vote = ProposalVote::new(proposal.hash(), &private_key);

        assert_eq!(
            flood_events[0],
            FloodEvent {
                message: Message::SnapshotProposalVote(expected_proposal_vote),
                traffic_type: TrafficType::LedgerSnapshots,
                scale: 0.0,
                all_prs: true,
            }
        );
    }

    #[test]
    fn publish_proposal_vote_only_once() {
        let mut rep_weights = RepWeights::new();
        let private_key = get_test_key().unwrap();
        let quorum_weight = Amount::nano(100_000);
        rep_weights.insert(private_key.public_key(), quorum_weight);

        let fixture = Fixture::with_rep_weights(rep_weights, quorum_weight);

        let proposal1 = Proposal::new(vec![], &private_key);
        let proposal2 = Proposal::new(vec![], &PrivateKey::from(2));
        fixture.snapshots.receive_proposal(proposal1.clone());
        fixture.snapshots.receive_proposal(proposal2);

        let flood_events = fixture.flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood only one vote message");
    }

    #[test]
    fn vote_for_proposal_with_highest_hash() {
        let proposal1 = Proposal::new(vec![], &PrivateKey::from(1));
        let proposal2 = Proposal::new(vec![], &PrivateKey::from(2));
        let proposal3 = Proposal::new(vec![], &PrivateKey::from(3));
        let proposal4 = Proposal::new(vec![], &PrivateKey::from(4));

        let highest_hash = [
            proposal1.hash(),
            proposal2.hash(),
            proposal3.hash(),
            proposal4.hash(),
        ]
        .into_iter()
        .max()
        .unwrap();

        let mut proposal_aggregator = Aggregator::<Proposal>::default();
        proposal_aggregator.add(proposal1);
        proposal_aggregator.add(proposal2);
        proposal_aggregator.add(proposal3);
        proposal_aggregator.add(proposal4);

        let proposal_vote =
            LedgerSnapshots::create_proposal_vote(&proposal_aggregator, &PrivateKey::from(5));

        assert_eq!(proposal_vote.unwrap().proposal_hash, highest_hash);
    }

    #[test]
    fn can_track_received_proposal_votes() {
        let fixture = Fixture::new();
        let proposal_vote = ProposalVote::new_test_instance();
        fixture
            .snapshots
            .receive_proposal_vote(proposal_vote.clone());

        let receive_events = fixture.receive_proposal_vote_tracker.output();
        assert_eq!(receive_events.len(), 1, "Should receive proposal vote");
        assert_eq!(receive_events[0], proposal_vote);
    }

    #[test]
    fn a_received_proposal_vote_is_added_to_the_proposal_vote_aggregator() {
        let fixture = Fixture::new();
        let snapshots = &fixture.snapshots;
        let proposal_vote = ProposalVote::new_test_instance();

        snapshots.receive_proposal_vote(proposal_vote.clone());

        assert!(
            snapshots
                .proposal_vote_aggregator
                .lock()
                .unwrap()
                .contains(&proposal_vote.hash())
        );
    }

    #[test]
    fn a_winner_proposal_is_not_found_if_there_are_no_votes() {
        assert_eq!(
            LedgerSnapshots::find_winner_proposal(&ConsensusParams::default(), vec![]),
            None
        );
    }

    #[test]
    fn a_winner_proposal_is_not_found_if_quorum_is_not_reached() {
        let rep_key = PrivateKey::from(1);
        let weight = Amount::nano(100_000);
        let mut rep_weights = RepWeights::new();
        rep_weights.insert(rep_key.public_key(), weight);
        let params = ConsensusParams { rep_weights, quorum_weight: Amount::MAX };

        let proposal_hash = ProposalHash::from(1);
        let proposal_vote = ProposalVote::new(proposal_hash, &rep_key);

        assert_eq!(LedgerSnapshots::find_winner_proposal(&params, &[proposal_vote]), None);
    }

    #[test]
    fn a_winner_proposal_is_found_if_quorum_is_reached() {
        let rep_key1 = PrivateKey::from(1);
        let rep_key2 = PrivateKey::from(2);
        let weight = Amount::nano(100_000);

        let mut rep_weights = RepWeights::new();
        rep_weights.insert(rep_key1.public_key(), weight);
        rep_weights.insert(rep_key2.public_key(), weight);
        let quorum_weight = weight * 2;

        let params = ConsensusParams { rep_weights, quorum_weight };

        let proposal_hash = ProposalHash::from(1);
        let proposal_vote1 = ProposalVote::new(proposal_hash, &rep_key1);
        let proposal_vote2 = ProposalVote::new(proposal_hash, &rep_key2);

        assert_eq!(
            LedgerSnapshots::find_winner_proposal(&params, &[proposal_vote1, proposal_vote2]),
            Some(proposal_hash)
        );
    }

    struct Fixture {
        snapshots: LedgerSnapshots,
        flood_tracker: Arc<OutputTrackerMt<FloodEvent>>,
        receive_preproposal_tracker: Arc<OutputTrackerMt<Preproposal>>,
        receive_proposal_tracker: Arc<OutputTrackerMt<Proposal>>,
        receive_proposal_vote_tracker: Arc<OutputTrackerMt<ProposalVote>>,
    }

    impl Fixture {
        fn new() -> Self {
            Self::with_frontiers([])
        }

        fn with_frontiers(frontiers: impl IntoIterator<Item = (Account, BlockHash)>) -> Self {
            let ledger = create_ledger_with_frontiers(frontiers);
            Self::with_ledger(ledger)
        }

        fn with_ledger(ledger: Arc<Ledger>) -> Self {
            Self::with_ledger_and_weights(ledger, RepWeights::new(), Amount::nano(60_000_000))
        }

        fn with_rep_weights(rep_weights: RepWeights, quorum_weight: Amount) -> Self {
            let ledger = create_ledger_with_frontiers([]);
            Self::with_ledger_and_weights(ledger, rep_weights, quorum_weight)
        }

        fn with_ledger_and_weights(
            ledger: Arc<Ledger>,
            rep_weights: RepWeights,
            quorum_weight: Amount,
        ) -> Self {
            let flooder = MessageFlooder::new_null();
            let flood_tracker = flooder.track_floods();

            let mut online_reps = OnlineReps::new(
                Arc::new(rep_weights.into()),
                Duration::ZERO,
                Amount::ZERO,
                Amount::ZERO,
            );
            online_reps.set_trended(quorum_weight / ONLINE_WEIGHT_QUORUM as u128 * 100);
            let online_reps = Arc::new(Mutex::new(online_reps));

            let snapshots =
                LedgerSnapshots::new(ledger.clone(), get_test_key, flooder, online_reps);

            let receive_preproposal_tracker = snapshots.track_received_preproposals();
            let receive_proposal_tracker = snapshots.track_received_proposals();
            let receive_proposal_vote_tracker = snapshots.track_received_proposal_votes();

            Self {
                snapshots,
                flood_tracker,
                receive_preproposal_tracker,
                receive_proposal_tracker,
                receive_proposal_vote_tracker,
            }
        }
    }

    fn get_test_key() -> Option<PrivateKey> {
        Some(PrivateKey::from(123))
    }

    fn create_ledger_with_frontiers(
        frontiers: impl IntoIterator<Item = (Account, BlockHash)>,
    ) -> Arc<Ledger> {
        let mut builder = Ledger::new_null_builder();

        for (account, frontier) in frontiers {
            builder = builder
                .account_info(&account, &AccountInfo::new_test_instance())
                .confirmation_height(
                    &account,
                    &ConfirmationHeightInfo {
                        height: 0,
                        frontier,
                    },
                );
        }

        builder.finish().into()
    }
}
