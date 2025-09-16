use crate::{ledger_snapshots::Aggregator, representatives::OnlineReps, transport::MessageFlooder};
use rsnano_messages::{Aggregatable, Message, Proposal, ProposalHash, ProposalVote};
use rsnano_network::TrafficType;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Amount, PrivateKey};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Consensus {
    /// For simplicity we currently assume that there is at most
    /// one representative key!
    /// TODO: We have to extend this later to multiple representatives per node.
    get_private_key: Arc<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
    flooder: Arc<Mutex<MessageFlooder>>,
    online_reps: Arc<Mutex<OnlineReps>>,
    receive_proposal_listener: OutputListenerMt<Proposal>,
    proposal_aggregator: Mutex<Aggregator<Proposal>>,
    receive_proposal_vote_listener: OutputListenerMt<ProposalVote>,
    proposal_vote_aggregator: Mutex<Aggregator<ProposalVote>>,
    proposal_voted: AtomicBool,
}

impl Consensus {
    pub fn new(
        get_private_key: Arc<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
        flooder: Arc<Mutex<MessageFlooder>>,
        online_reps: Arc<Mutex<OnlineReps>>,
    ) -> Self {
        Self {
            get_private_key,
            flooder,
            online_reps,
            receive_proposal_listener: OutputListenerMt::default(),
            proposal_aggregator: Default::default(),
            receive_proposal_vote_listener: OutputListenerMt::default(),
            proposal_vote_aggregator: Default::default(),
            proposal_voted: AtomicBool::new(false),
        }
    }

    pub fn new_null() -> Self {
        Self::new(
            Arc::new(|| None),
            Mutex::new(MessageFlooder::new_null()).into(),
            Mutex::new(OnlineReps::new_test_instance()).into(),
        )
    }

    pub fn receive_proposal(&self, proposal: Proposal) {
        self.receive_proposal_listener.emit(proposal.clone());

        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

        let has_quorum = {
            let mut proposal_aggregator = self.proposal_aggregator.lock().unwrap();
            proposal_aggregator.add(proposal);
            proposal_aggregator.has_quorum(&consensus_params)
        };

        if has_quorum && !self.proposal_voted.load(Ordering::SeqCst) {
            self.proposal_voted.store(true, Ordering::SeqCst);

            if let Some(proposal_vote) = self.create_proposal_vote() {
                self.flooder.lock().unwrap().flood_prs_and_some_non_prs(
                    &Message::SnapshotProposalVote(proposal_vote),
                    TrafficType::LedgerSnapshots,
                    0.0,
                );
            }
        }
    }

    fn create_proposal_vote(&self) -> Option<ProposalVote> {
        Some(ProposalVote::new(
            self.proposal_aggregator
                .lock()
                .unwrap()
                .values()
                .map(|p| p.hash())
                .max()?,
            &(self.get_private_key)().unwrap(),
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

        {
            let mut vote_aggregator = self.proposal_vote_aggregator.lock().unwrap();
            vote_aggregator.add(proposal_vote);
        }

        if let Some(winner) = self.find_winner_proposal() {
            tracing::warn!(proposal_hash=?winner, "Found a winner!");
        }
    }

    pub(crate) fn find_winner_proposal<'a>(&self) -> Option<ProposalHash> {
        let aggregator = self.proposal_vote_aggregator.lock().unwrap();
        let votes = aggregator.values();
        let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();
        let mut tallies: HashMap<ProposalHash, Amount> = HashMap::new();

        for vote in votes.into_iter() {
            let weight: &mut Amount = tallies.entry(vote.proposal_hash).or_default();
            *weight += consensus_params.rep_weights.weight(&vote.voter);
        }

        tallies
            .into_iter()
            .find(|(_, w)| *w >= consensus_params.quorum_weight)
            .map(|(p, _)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        representatives::{ConsensusParams, ONLINE_WEIGHT_QUORUM},
        transport::FloodEvent,
    };
    use rsnano_ledger::RepWeights;
    use rsnano_messages::{Aggregatable, Message, ProposalVote};
    use rsnano_network::TrafficType;
    use rsnano_output_tracker::OutputTrackerMt;
    use rsnano_types::{Account, AccountInfo, Amount, BlockHash, ConfirmationHeightInfo};
    use std::time::Duration;

    #[test]
    fn can_track_received_proposals() {
        let consensus = Consensus::new_null();
        let proposal = Proposal::new_test_instance();
        let receive_proposal_tracker = consensus.track_received_proposals();

        consensus.receive_proposal(proposal.clone());
        let receive_events = receive_proposal_tracker.output();

        assert_eq!(receive_events.len(), 1, "Should receive proposal");
        assert_eq!(receive_events[0], proposal);
    }

    #[test]
    fn a_received_proposal_is_added_to_the_proposal_aggregator() {
        let consensus = Consensus::new_null();
        let proposal = Proposal::new_test_instance();

        consensus.receive_proposal(proposal.clone());

        assert!(
            consensus
                .proposal_aggregator
                .lock()
                .unwrap()
                .contains(&proposal.hash())
        );
    }

    #[test]
    fn publish_proposal_vote_when_quorum_of_proposals_is_reached() {
        let private_key = get_private_key().unwrap();

        let mut rep_weights: RepWeights = RepWeights::new();
        let quorum_weight = Amount::nano(100_000);

        rep_weights.insert(private_key.public_key(), quorum_weight);
        let params = &ConsensusParams {
            rep_weights,
            quorum_weight,
        };

        let consensus = create_consensus_with_params(params);
        let flood_tracker: Arc<OutputTrackerMt<FloodEvent>> =
            consensus.flooder.lock().unwrap().track_floods();

        let proposal = Proposal::new(vec![], &private_key);
        consensus.receive_proposal(proposal.clone());

        let flood_events = flood_tracker.output();

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
        let private_key = get_private_key().unwrap();

        let mut rep_weights = RepWeights::new();
        let quorum_weight = Amount::nano(100_000);

        rep_weights.insert(private_key.public_key(), quorum_weight);
        let params = &ConsensusParams {
            rep_weights,
            quorum_weight,
        };

        let consensus = create_consensus_with_params(params);
        let flood_tracker = consensus.flooder.lock().unwrap().track_floods();

        let proposal1 = Proposal::new(vec![], &private_key);
        let proposal2 = Proposal::new(vec![], &PrivateKey::from(2));

        consensus.receive_proposal(proposal1.clone());
        consensus.receive_proposal(proposal2);

        let flood_events = flood_tracker.output();

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

        let consensus = Consensus::new(
            Arc::new(get_private_key),
            Mutex::new(MessageFlooder::new_null()).into(),
            Mutex::new(OnlineReps::new_test_instance()).into(),
        );

        {
            let mut proposal_aggregator = consensus.proposal_aggregator.lock().unwrap();

            proposal_aggregator.add(proposal1);
            proposal_aggregator.add(proposal2);
            proposal_aggregator.add(proposal3);
            proposal_aggregator.add(proposal4);
        }

        let proposal_vote = consensus.create_proposal_vote();

        assert_eq!(proposal_vote.unwrap().proposal_hash, highest_hash);
    }

    #[test]
    fn can_track_received_proposal_votes() {
        let consensus = Consensus::new_null();
        let proposal_vote = ProposalVote::new_test_instance();

        let receive_proposal_vote_tracker = consensus.track_received_proposal_votes();

        consensus.receive_proposal_vote(proposal_vote.clone());

        let receive_events = receive_proposal_vote_tracker.output();

        assert_eq!(receive_events.len(), 1, "Should receive proposal vote");
        assert_eq!(receive_events[0], proposal_vote);
    }

    #[test]
    fn a_received_proposal_vote_is_added_to_the_proposal_vote_aggregator() {
        let consensus = Consensus::new_null();
        let proposal_vote = ProposalVote::new_test_instance();

        consensus.receive_proposal_vote(proposal_vote.clone());

        assert!(
            consensus
                .proposal_vote_aggregator
                .lock()
                .unwrap()
                .contains(&proposal_vote.hash())
        );
    }

    #[test]
    fn a_winner_proposal_is_not_found_if_there_are_no_votes() {
        let consensus = Consensus::new_null();

        assert_eq!(consensus.find_winner_proposal(), None);
    }

    #[test]
    fn a_winner_proposal_is_not_found_if_quorum_is_not_reached() {
        let rep_key = PrivateKey::from(1);
        let weight = Amount::nano(100_000);
        let mut rep_weights = RepWeights::new();
        rep_weights.insert(rep_key.public_key(), weight);
        let params = ConsensusParams {
            rep_weights,
            quorum_weight: weight * 2,
        };

        let proposal_hash = ProposalHash::from(1);
        let proposal_vote = ProposalVote::new(proposal_hash, &rep_key);

        let consensus = create_consensus_with_params(&params);
        consensus
            .proposal_vote_aggregator
            .lock()
            .unwrap()
            .add(proposal_vote);

        assert_eq!(consensus.find_winner_proposal(), None);
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

        let params = ConsensusParams {
            rep_weights,
            quorum_weight,
        };

        let proposal_hash = ProposalHash::from(1);
        let proposal_vote1 = ProposalVote::new(proposal_hash, &rep_key1);
        let proposal_vote2 = ProposalVote::new(proposal_hash, &rep_key2);

        let consensus = create_consensus_with_params(&params);

        {
            let mut vote_aggregator = consensus.proposal_vote_aggregator.lock().unwrap();
            vote_aggregator.add(proposal_vote1);
            vote_aggregator.add(proposal_vote2);
        }

        assert_eq!(consensus.find_winner_proposal(), Some(proposal_hash));
    }

    fn create_consensus_with_params(params: &ConsensusParams) -> Consensus {
        let flooder = MessageFlooder::new_null();

        let mut online_reps = OnlineReps::new(
            Arc::new(params.rep_weights.clone().into()),
            Duration::ZERO,
            Amount::ZERO,
            Amount::ZERO,
        );
        online_reps.set_trended(params.quorum_weight / ONLINE_WEIGHT_QUORUM as u128 * 100);
        let online_reps = Arc::new(Mutex::new(online_reps));

        Consensus::new(
            Arc::new(get_private_key),
            Mutex::new(flooder).into(),
            online_reps,
        )
    }

    fn get_private_key() -> Option<PrivateKey> {
        Some(PrivateKey::from(123))
    }
}
