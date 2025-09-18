mod aggregator;
mod ledger_snapshots_state;

use crate::{
    ledger_snapshots::{aggregator::Aggregator, ledger_snapshots_state::LedgerSnapshotsState},
    representatives::OnlineReps,
    transport::MessageFlooder,
};
use rsnano_ledger::Ledger;
use rsnano_messages::{Aggregatable, Message, Preproposal, Proposal, ProposalVote};
use rsnano_network::TrafficType;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Account, BlockHash};
use rsnano_types::{PrivateKey, SnapshotNumber};
use std::sync::{Arc, Mutex};
use tracing::warn;

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
    state: Mutex<LedgerSnapshotsState>,
    online_reps: Arc<Mutex<OnlineReps>>,
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
            state: Default::default(),
            online_reps,
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

    pub fn new_test_instance(frontiers: impl IntoIterator<Item = (Account, BlockHash)>) -> Self {
        let private_key = PrivateKey::new_test_instance();
        let public_key = private_key.public_key();

        Self::new(
            Ledger::new_test_instance(frontiers),
            move || Some(private_key.clone()),
            MessageFlooder::new_null(),
            Mutex::new(OnlineReps::new_test_instance2(public_key)).into(),
        )
    }

    pub fn publish_preproposal(&self) {
        warn!("Preproposal generation triggered");
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
        Preproposal::new(frontiers, private_key, self.get_current_snapshot_number())
    }

    fn collect_frontiers(&self) -> Vec<(Account, BlockHash)> {
        self.ledger.confirmed().frontiers().collect()
    }

    pub fn receive_preproposal(&self, preproposal: Preproposal) {
        warn!(preproposal_hash= ?preproposal.hash(), "Snapshot preproposal received");
        self.receive_preproposal_listener.emit(preproposal.clone());
        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

        let mut state = self.state.lock().unwrap();
        if !state.receive_preproposal(preproposal.clone()) {
            warn!(preproposal_hash= ?preproposal.hash(), snapshot_number= ?preproposal.snapshot_number, "Snapshot preproposal discarded because snapshot number is different than current");
            return;
        }

        warn!(
            "Current preproposal tally = {:?}",
            state.preproposal_aggregator.tally(&consensus_params)
        );

        let rep_key = (self.get_private_key)().unwrap();
        let proposal = state.try_create_proposal(&consensus_params, &rep_key);
        if proposal.is_some() {
            warn!("Quorum on preproposals reached");
        } else {
            warn!("No quorum on preproposals yet");
        }
        drop(state);

        if let Some(proposal) = proposal {
            warn!(proposal_hash = ?proposal.hash(), "Created proposal. Flooding...");
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
        warn!(proposal_hash = ?proposal.hash(), "Snapshot proposal received");
        self.receive_proposal_listener.emit(proposal.clone());
        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

        let mut state = self.state.lock().unwrap();
        if !state.receive_proposal(proposal.clone()) {
            warn!(proposal_hash= ?proposal.hash(), snapshot_number= ?proposal.snapshot_number, "Snapshot proposal discarded because snapshot number is different than current");
            return;
        }

        warn!(
            "Current proposal tally = {:?}",
            state.proposal_aggregator.tally(&consensus_params)
        );

        let rep_key = (self.get_private_key)().unwrap();
        if let Some(vote) = state.try_create_vote(&consensus_params, &rep_key) {
            warn!("Quorum on proposal reached");
            warn!(vote_hash = ?vote.hash(), "Flooding proposal vote");
            self.flooder.lock().unwrap().flood_prs_and_some_non_prs(
                &Message::SnapshotProposalVote(vote),
                TrafficType::LedgerSnapshots,
                0.0,
            );
        }
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
        let mut state = self.state.lock().unwrap();

        if !state.receive_vote(proposal_vote.clone(), &consensus_params) {
            warn!(proposal_vote_hash= ?proposal_vote.hash(), snapshot_number= ?proposal_vote.snapshot_number, "Snapshot proposal vote discarded because snapshot number is different than current");
            return;
        }

        warn!(
            received_votes = state.vote_aggregator.len(),
            "Snapshot proposal vote received"
        );
    }

    fn get_current_snapshot_number(&self) -> SnapshotNumber {
        self.state.lock().unwrap().current_snapshot_number
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FloodEvent;
    use rsnano_messages::{Aggregatable, Message, ProposalVote};
    use rsnano_network::TrafficType;

    #[test]
    fn collect_one_frontier() {
        let account = Account::from(1);
        let frontier = BlockHash::from(2);
        let ledger_snapshots = LedgerSnapshots {
            ledger: Ledger::new_test_instance([(account, frontier)]),
            ..LedgerSnapshots::new_null()
        };

        assert_eq!(ledger_snapshots.collect_frontiers(), [(account, frontier)]);
    }

    #[test]
    fn collect_multiple_frontiers() {
        let account1 = Account::from(1);
        let frontier1 = BlockHash::from(100);
        let account2 = Account::from(2);
        let frontier2 = BlockHash::from(200);
        let ledger_snapshots = LedgerSnapshots {
            ledger: Ledger::new_test_instance([(account1, frontier1), (account2, frontier2)]),
            ..LedgerSnapshots::new_null()
        };

        assert_eq!(
            ledger_snapshots.collect_frontiers(),
            [(account1, frontier1), (account2, frontier2)]
        );
    }

    #[test]
    fn create_preproposal_with_one_frontier() {
        let account = Account::from(1);
        let frontier = BlockHash::from(2);
        let ledger_snapshots = LedgerSnapshots::new_test_instance([(account, frontier)]);
        let preproposal = ledger_snapshots.create_preproposal(&PrivateKey::new_test_instance());

        assert!(preproposal.frontiers.contains(&(account, frontier)));
        assert_eq!(
            preproposal.snapshot_number,
            ledger_snapshots.get_current_snapshot_number()
        );
    }

    #[test]
    fn publish_preproposal() {
        let ledger_snapshots = LedgerSnapshots::new_test_instance([]);
        let flood_tracker = ledger_snapshots.flooder.lock().unwrap().track_floods();

        ledger_snapshots.publish_preproposal();
        let flood_events = flood_tracker.output();
        let expected_preproposal =
            ledger_snapshots.create_preproposal(&PrivateKey::new_test_instance());

        assert_eq!(flood_events.len(), 1, "Should flood the message");
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
        let ledger_snapshots = LedgerSnapshots::new_null();
        let receive_preproposal_tracker = ledger_snapshots.track_received_preproposals();
        let preproposal = Preproposal::new_test_instance();

        ledger_snapshots.receive_preproposal(preproposal.clone());
        let receive_events = receive_preproposal_tracker.output();

        assert_eq!(receive_events.len(), 1, "Should receive preproposal");
        assert_eq!(receive_events[0], preproposal);
    }

    #[test]
    fn publish_proposal_when_quorum_of_preproposals_is_reached() {
        let private_key = PrivateKey::new_test_instance();
        let ledger_snapshots = LedgerSnapshots::new_test_instance([]);
        let flood_tracker = ledger_snapshots.flooder.lock().unwrap().track_floods();
        let snapshot_number = ledger_snapshots.get_current_snapshot_number();

        let preproposal = Preproposal::new(vec![], &private_key, snapshot_number);
        ledger_snapshots.receive_preproposal(preproposal.clone());

        let flood_events = flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let snapshot_number = ledger_snapshots.get_current_snapshot_number();
        let expected_proposal = Proposal::new(&[preproposal], &private_key, snapshot_number);

        assert_eq!(
            flood_events[0],
            FloodEvent {
                message: Message::SnapshotProposal(expected_proposal),
                traffic_type: TrafficType::LedgerSnapshots,
                scale: 0.0,
                all_prs: true,
            }
        );

        assert_eq!(
            snapshot_number,
            ledger_snapshots.get_current_snapshot_number()
        );
    }

    #[test]
    fn can_track_received_proposals() {
        let ledger_snapshots = LedgerSnapshots::new_null();
        let proposal = Proposal::new_test_instance();
        let receive_proposal_tracker = ledger_snapshots.track_received_proposals();
        ledger_snapshots.receive_proposal(proposal.clone());

        let receive_events = receive_proposal_tracker.output();
        assert_eq!(receive_events.len(), 1, "Should receive proposal");
        assert_eq!(receive_events[0], proposal);
    }

    #[test]
    fn publish_proposal_vote_when_quorum_of_proposals_is_reached() {
        let private_key = PrivateKey::new_test_instance();
        let ledger_snapshots = LedgerSnapshots::new_test_instance([]);
        let flood_tracker = ledger_snapshots.flooder.lock().unwrap().track_floods();
        let snapshot_number = ledger_snapshots.get_current_snapshot_number();

        let proposal = Proposal::new(vec![], &private_key, snapshot_number);
        ledger_snapshots.receive_proposal(proposal.clone());

        let flood_events = flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let expected_proposal_vote =
            ProposalVote::new(proposal.hash(), &private_key, snapshot_number);

        assert_eq!(
            flood_events[0],
            FloodEvent {
                message: Message::SnapshotProposalVote(expected_proposal_vote),
                traffic_type: TrafficType::LedgerSnapshots,
                scale: 0.0,
                all_prs: true,
            }
        );

        assert_eq!(
            snapshot_number,
            ledger_snapshots.get_current_snapshot_number()
        );
    }

    #[test]
    fn publish_proposal_vote_only_once() {
        let private_key = PrivateKey::new_test_instance();
        let ledger_snapshots = LedgerSnapshots::new_test_instance([]);
        let flood_tracker = ledger_snapshots.flooder.lock().unwrap().track_floods();
        let snapshot_number = ledger_snapshots.get_current_snapshot_number();

        let proposal1 = Proposal::new(vec![], &private_key, snapshot_number);
        let proposal2 = Proposal::new(vec![], &PrivateKey::from(2), snapshot_number);
        ledger_snapshots.receive_proposal(proposal1.clone());
        ledger_snapshots.receive_proposal(proposal2);

        let flood_events = flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood only one vote message");
    }

    #[test]
    fn can_track_received_proposal_votes() {
        let ledger_snapshots = LedgerSnapshots::new_null();
        let receive_proposal_vote_tracker = ledger_snapshots.track_received_proposal_votes();
        let proposal_vote = ProposalVote::new_test_instance();
        ledger_snapshots.receive_proposal_vote(proposal_vote.clone());

        let receive_events = receive_proposal_vote_tracker.output();
        assert_eq!(receive_events.len(), 1, "Should receive proposal vote");
        assert_eq!(receive_events[0], proposal_vote);
    }

    #[test]
    fn initial_snapshot_number_should_be_zero() {
        let ledger_snapshots = LedgerSnapshots::new_null();

        assert_eq!(ledger_snapshots.get_current_snapshot_number(), 0);
    }
}
