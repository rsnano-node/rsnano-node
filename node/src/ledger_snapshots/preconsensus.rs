use crate::{ledger_snapshots::Aggregator, representatives::OnlineReps, transport::MessageFlooder};
use rsnano_ledger::Ledger;
use rsnano_messages::{Message, Preproposal, Proposal};
use rsnano_network::TrafficType;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Account, BlockHash, PrivateKey};
use std::sync::{Arc, Mutex};

pub struct Preconsensus {
    ledger: Arc<Ledger>,
    /// For simplicity we currently assume that there is at most
    /// one representative key!
    /// TODO: We have to extend this later to multiple representatives per node.
    get_private_key: Arc<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
    flooder: Arc<Mutex<MessageFlooder>>,
    receive_preproposal_listener: OutputListenerMt<Preproposal>,
    online_reps: Arc<Mutex<OnlineReps>>,
    preproposal_aggregator: Mutex<Aggregator<Preproposal>>,
}

impl Preconsensus {
    pub fn new(
        ledger: Arc<Ledger>,
        get_private_key: Arc<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
        flooder: Arc<Mutex<MessageFlooder>>,
        online_reps: Arc<Mutex<OnlineReps>>,
    ) -> Self {
        Self {
            ledger,
            get_private_key,
            flooder,
            online_reps,
            receive_preproposal_listener: OutputListenerMt::default(),
            preproposal_aggregator: Default::default(),
        }
    }

    pub fn new_null() -> Self {
        Self::new(
            Ledger::new_null().into(),
            Arc::new(|| None),
            Mutex::new(MessageFlooder::new_null()).into(),
            Mutex::new(OnlineReps::new_test_instance()).into(),
        )
    }

    fn collect_frontiers(&self) -> Vec<(Account, BlockHash)> {
        self.ledger.confirmed().frontiers().collect()
    }

    fn create_preproposal(&self, private_key: &PrivateKey) -> Preproposal {
        let frontiers = self.collect_frontiers();
        Preproposal::new(frontiers, private_key)
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

    pub fn receive_preproposal(&self, preproposal: Preproposal) {
        self.receive_preproposal_listener.emit(preproposal.clone());

        let proposal = {
            let mut preproposal_aggregator = self.preproposal_aggregator.lock().unwrap();
            preproposal_aggregator.add(preproposal);

            let consensus_params = self.online_reps.lock().unwrap().get_consensus_params();

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
}

mod tests {
    use super::*;
    use crate::{
        representatives::{ConsensusParams, ONLINE_WEIGHT_QUORUM},
        transport::FloodEvent,
    };
    use rsnano_ledger::RepWeights;
    use rsnano_messages::Aggregatable;
    use rsnano_types::{AccountInfo, Amount, ConfirmationHeightInfo};
    use std::time::Duration;

    #[test]
    fn ledger_with_one_account() {
        let account = Account::from(1);
        let frontier = BlockHash::from(2);
        let preconsensus = create_preconsensus_with_frontiers([(account, frontier)]);
        assert_eq!(preconsensus.collect_frontiers(), [(account, frontier)]);
    }

    #[test]
    fn ledger_with_multiple_accounts() {
        let account1 = Account::from(1);
        let frontier1 = BlockHash::from(100);
        let account2 = Account::from(2);
        let frontier2 = BlockHash::from(200);

        let preconsensus =
            create_preconsensus_with_frontiers([(account1, frontier1), (account2, frontier2)]);
        assert_eq!(
            preconsensus.collect_frontiers(),
            [(account1, frontier1), (account2, frontier2)]
        );
    }

    #[test]
    fn create_preproposal() {
        let account = Account::from(10);
        let frontier = BlockHash::from(2);
        let preconsensus = create_preconsensus_with_frontiers([(account, frontier)]);

        let preproposal = preconsensus.create_preproposal(&get_private_key().unwrap());

        assert!(preproposal.frontiers.contains(&(account, frontier)));
    }

    #[test]
    fn publish_preproposal() {
        let account = Account::from(1);
        let frontier = BlockHash::from(100);
        let preconsensus = create_preconsensus_with_frontiers([(account, frontier)]);
        let flood_tracker = preconsensus.flooder.lock().unwrap().track_floods();

        preconsensus.publish_preproposal();

        let flood_events = flood_tracker.output();
        assert_eq!(flood_events.len(), 1, "Should flood the message");

        let expected_preproposal = preconsensus.create_preproposal(&get_private_key().unwrap());

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
        let preconsensus = Preconsensus::new_null();
        let preproposal = Preproposal::new_test_instance();

        let receive_preproposal_tracker = preconsensus.track_received_preproposals();

        preconsensus.receive_preproposal(preproposal.clone());

        let receive_events: Vec<Preproposal> = receive_preproposal_tracker.output();

        assert_eq!(receive_events.len(), 1, "Should receive preproposal");
        assert_eq!(receive_events[0], preproposal);
    }

    #[test]
    fn a_received_preproposal_is_added_to_the_preproposal_aggregator() {
        let preconsensus = Preconsensus::new_null();
        let preproposal = Preproposal::new_test_instance();

        preconsensus.receive_preproposal(preproposal.clone());

        assert!(
            preconsensus
                .preproposal_aggregator
                .lock()
                .unwrap()
                .contains(&preproposal.hash())
        );
    }

    #[test]
    fn publish_proposal_when_quorum_of_preproposals_is_reached() {
        let mut rep_weights = RepWeights::new();
        let private_key = get_private_key().unwrap();
        let quorum_weight = Amount::nano(100_000);

        rep_weights.insert(private_key.public_key(), quorum_weight);
        let params = ConsensusParams {
            rep_weights,
            quorum_weight,
        };
        let preconsensus = create_preconsensus_with_params(&params);

        let preproposal = Preproposal::new(vec![], &private_key);
        let flood_tracker: Arc<OutputTrackerMt<FloodEvent>> =
            preconsensus.flooder.lock().unwrap().track_floods();

        preconsensus.receive_preproposal(preproposal.clone());

        let flood_events = flood_tracker.output();
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

    fn create_preconsensus_with_frontiers(
        frontiers: impl IntoIterator<Item = (Account, BlockHash)>,
    ) -> Preconsensus {
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

        let ledger = builder.finish().into();

        Preconsensus::new(
            ledger,
            Arc::new(get_private_key),
            Mutex::new(MessageFlooder::new_null()).into(),
            Mutex::new(OnlineReps::new_test_instance()).into(),
        )
    }

    fn create_preconsensus_with_params(params: &ConsensusParams) -> Preconsensus {
        let mut online_reps = OnlineReps::new(
            Arc::new(params.rep_weights.clone().into()),
            Duration::ZERO,
            Amount::ZERO,
            Amount::ZERO,
        );
        online_reps.set_trended(params.quorum_weight / ONLINE_WEIGHT_QUORUM as u128 * 100);
        let online_reps = Arc::new(Mutex::new(online_reps));

        Preconsensus::new(
            Ledger::new_null().into(),
            Arc::new(get_private_key),
            Mutex::new(MessageFlooder::new_null()).into(),
            online_reps,
        )
    }

    fn get_private_key() -> Option<PrivateKey> {
        Some(PrivateKey::from(123))
    }
}
