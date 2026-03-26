use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use rsnano_ledger::RepWeightCache;
use rsnano_nullable_clock::{SteadyClock, Timestamp};
use rsnano_types::{
    Amount, Block, BlockHash, BlockPriority, PublicKey, QualifiedRoot, SavedBlock, VoteError,
};
use rsnano_utils::sync::backpressure_channel::{self, Receiver, Sender};

use crate::{
    consensus::{
        ActiveElectionsConfig, ActiveElectionsContainer, AecCooldownReason, AecEvent,
        AecInsertError, AecInsertRequest, ApplyVoteArgs, FilteredVote, ReceivedVote,
        election::{ConfirmedElection, Election, ElectionBehavior},
    },
    representatives::OnlineReps,
    utils::{BackpressureEventProcessor, spawn_backpressure_processor},
};

pub(crate) struct AecService {
    active: Arc<RwLock<ActiveElectionsContainer>>,
    events_tx: Sender<AecEvent>,
    events_rx: Mutex<Option<Receiver<AecEvent>>>,
    online_reps: Arc<Mutex<OnlineReps>>,
    clock: Arc<SteadyClock>,
    rep_weights: Arc<RepWeightCache>,
    is_dev_network: bool,
}

impl AecService {
    const EVENT_QUEUE_SOFT_LIMIT: usize = 1024 * 5;

    pub(crate) fn new(
        config: ActiveElectionsConfig,
        base_latency: Duration,
        online_reps: Arc<Mutex<OnlineReps>>,
        clock: Arc<SteadyClock>,
        rep_weights: Arc<RepWeightCache>,
        is_dev_network: bool,
    ) -> Self {
        let (events_tx, events_rx) = backpressure_channel::channel(Self::EVENT_QUEUE_SOFT_LIMIT);

        let mut active = ActiveElectionsContainer::new(config, base_latency);
        active.set_observer(events_tx.clone());

        Self {
            active: Arc::new(RwLock::new(active)),
            events_tx,
            events_rx: Mutex::new(Some(events_rx)),
            online_reps,
            clock,
            rep_weights,
            is_dev_network,
        }
    }

    pub(crate) fn new_null() -> Self {
        let rep_weights = Arc::new(RepWeightCache::default());
        let online_reps = Arc::new(Mutex::new(
            OnlineReps::builder()
                .rep_weights(rep_weights.clone())
                .finish(),
        ));
        Self::new(
            ActiveElectionsConfig::default(),
            Duration::from_secs(1),
            online_reps,
            Arc::new(SteadyClock::new_null()),
            rep_weights,
            false,
        )
    }

    pub(crate) fn event_queue_len(&self) -> usize {
        self.events_tx.len()
    }

    pub(crate) fn start_event_processor<T>(&self, thread_name: impl Into<String>, processor: T)
    where
        T: BackpressureEventProcessor<AecEvent> + Send + 'static,
    {
        let receiver = self
            .events_rx
            .lock()
            .unwrap()
            .take()
            .expect("AEC event processor already started");

        spawn_backpressure_processor(thread_name, receiver, processor);
    }

    pub(crate) fn set_cooldown(&self, cool_down: bool, reason: AecCooldownReason) {
        self.active.write().unwrap().set_cooldown(cool_down, reason);
    }

    pub(crate) fn erase(&self, root: &QualifiedRoot) -> bool {
        self.active.write().unwrap().erase(root)
    }

    pub(crate) fn max_len(&self) -> usize {
        self.active.read().unwrap().max_len()
    }

    pub(crate) fn vacancy(&self) -> i64 {
        self.active.read().unwrap().vacancy()
    }

    pub(crate) fn info(&self) -> crate::consensus::ActiveElectionsInfo {
        self.active.read().unwrap().info()
    }

    pub(crate) fn was_recently_confirmed(&self, block_hash: &BlockHash) -> bool {
        self.active.read().unwrap().was_recently_confirmed(block_hash)
    }

    pub(crate) fn elections_round_robin(&self) -> Vec<Election> {
        self.active
            .read()
            .unwrap()
            .iter_round_robin()
            .cloned()
            .collect()
    }

    pub(crate) fn now(&self) -> Timestamp {
        self.clock.now()
    }

    pub(crate) fn count_by_behavior(&self, behavior: ElectionBehavior) -> usize {
        self.active.read().unwrap().count_by_behavior(behavior)
    }

    #[cfg(test)]
    pub(crate) fn is_active_hash(&self, hash: &BlockHash) -> bool {
        self.active.read().unwrap().is_active_hash(hash)
    }

    pub(crate) fn remove_recently_confirmed(&self, block_hash: &BlockHash) {
        self.active
            .write()
            .unwrap()
            .remove_recently_confirmed(block_hash);
    }

    pub(crate) fn confirm_dependent_elections(
        &self,
        confirmed: Vec<(SavedBlock, Option<ConfirmedElection>)>,
    ) {
        self.active
            .write()
            .unwrap()
            .confirm_dependent_elections(confirmed, self.clock.now());
    }

    pub(crate) fn try_add_fork(&self, fork: &Block, fork_tally: Amount) -> bool {
        self.active.write().unwrap().try_add_fork(fork, fork_tally)
    }

    pub(crate) fn transition_time(&self) {
        self.active
            .write()
            .unwrap()
            .transition_time(self.clock.now());
    }

    pub(crate) fn transition_active(&self, block_hash: &BlockHash) -> bool {
        self.active.write().unwrap().transition_active(block_hash)
    }

    pub(crate) fn remove_votes(
        &self,
        root: &QualifiedRoot,
        voters: impl IntoIterator<Item = PublicKey>,
    ) {
        let voters = voters.into_iter().collect::<Vec<_>>();
        self.active
            .write()
            .unwrap()
            .remove_votes(root, voters.iter());
    }

    pub(crate) fn activate_manual(&self, block: SavedBlock, priority: BlockPriority) -> bool {
        let hash = block.hash();
        let mut active = self.active.write().unwrap();
        if active
            .insert(
                AecInsertRequest::new_manual(block, priority),
                self.clock.now(),
            )
            .is_ok()
        {
            active.transition_active(&hash);
            true
        } else {
            false
        }
    }

    pub(crate) fn insert_hinted(&self, block: SavedBlock, priority: BlockPriority) -> bool {
        self.active
            .write()
            .unwrap()
            .insert(
                AecInsertRequest::new_hinted(block, priority),
                self.clock.now(),
            )
            .is_ok()
    }

    pub(crate) fn insert_optimistic(&self, block: SavedBlock, priority: BlockPriority) -> bool {
        self.active
            .write()
            .unwrap()
            .insert(
                AecInsertRequest::new_optimistic(block, priority),
                self.clock.now(),
            )
            .is_ok()
    }

    pub(crate) fn insert_priority(
        &self,
        block: SavedBlock,
        priority: BlockPriority,
    ) -> Result<(), AecInsertError> {
        self.active.write().unwrap().insert(
            AecInsertRequest::new_priority(block, priority),
            self.clock.now(),
        )
    }

    pub(crate) fn bucket_len(&self, bucket: usize) -> usize {
        self.active.read().unwrap().bucket_len(bucket)
    }

    pub(crate) fn lowest_priority(
        &self,
        bucket: usize,
    ) -> Option<(QualifiedRoot, rsnano_types::TimePriority)> {
        self.active.read().unwrap().lowest_priority(bucket)
    }

    pub(crate) fn find_bucket(&self, root: &QualifiedRoot) -> Option<usize> {
        self.active.read().unwrap().find_bucket(root)
    }

    pub(crate) fn erase_lowest_prio_election(&self, bucket: usize) {
        self.active
            .write()
            .unwrap()
            .erase_lowest_prio_election(bucket);
    }

    pub(crate) fn apply_vote(
        &self,
        vote: &FilteredVote,
    ) -> HashMap<BlockHash, Result<(), VoteError>> {
        debug_assert!(vote.validate().is_ok());

        let minimum_pr_weight = self.online_reps.lock().unwrap().minimum_principal_weight();
        let voter_weight = self.rep_weights.weight(&vote.voter);

        if !self.is_dev_network && voter_weight <= minimum_pr_weight {
            return vote
                .filtered_blocks()
                .map(|hash| (*hash, Err(VoteError::Indeterminate)))
                .collect();
        }

        let is_active = {
            let active = self.active.read().unwrap();
            vote.filtered_blocks()
                .any(|hash| active.is_active_hash(hash))
        };

        let now = self.clock.now();
        let quorum_specs = {
            let mut online = self.online_reps.lock().unwrap();
            if is_active {
                online.vote_observed(vote.voter, now);
            }
            online.quorum_specs()
        };

        let results = {
            let mut active = self.active.write().unwrap();
            let rep_weights = self.rep_weights.read();
            active.apply_vote(ApplyVoteArgs {
                vote,
                rep_weights: &rep_weights,
                quorum_specs: &quorum_specs,
                now,
            })
        };

        self.notify_vote_processed(vote.vote.clone(), voter_weight, &results);
        results
    }

    fn notify_vote_processed(
        &self,
        vote: ReceivedVote,
        voter_weight: Amount,
        results: &HashMap<BlockHash, Result<(), VoteError>>,
    ) {
        let _ = self
            .events_tx
            .send(AecEvent::VoteProcessed(vote, voter_weight, results.clone()));
    }

    /// Temporary compatibility bridge for collaborators that have not yet migrated to AecService.
    pub(crate) fn legacy_container(&self) -> Arc<RwLock<ActiveElectionsContainer>> {
        Arc::clone(&self.active)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::consensus::{AecEvent, AecInsertRequest};
    use rsnano_types::{
        BlockPriority, PrivateKey, SavedBlock, UnixMillisTimestamp, Vote, VoteSource,
    };
    use std::sync::mpsc::TryRecvError;

    use super::*;

    #[test]
    fn construction_does_not_start_processing_implicitly() {
        let service = AecService::new_null();

        service
            .legacy_container()
            .read()
            .unwrap()
            .simulate_event(AecEvent::Recovered);

        assert_eq!(service.event_queue_len(), 1);
    }

    #[test]
    fn processing_starts_only_when_explicitly_requested() {
        let service = AecService::new_null();
        let processor = StubProcessor::default();

        service
            .legacy_container()
            .read()
            .unwrap()
            .simulate_event(AecEvent::Recovered);
        assert!(processor.log().is_empty());

        service.start_event_processor("aec-service-test", processor.clone());

        let start = std::time::Instant::now();
        while processor.log().is_empty() && start.elapsed() < Duration::from_secs(5) {
            std::thread::yield_now();
        }

        assert_eq!(processor.log(), vec!["processed"]);
    }

    #[test]
    fn apply_vote_publishes_vote_processed_event() {
        let service = AecService::new_null();
        let rep_key = PrivateKey::from(1);
        let block = SavedBlock::new_test_instance();
        let block_hash = block.hash();

        service.rep_weights.put(rep_key.public_key(), Amount::MAX);
        service
            .legacy_container()
            .write()
            .unwrap()
            .insert(
                AecInsertRequest::new_priority(block, BlockPriority::new_test_instance()),
                service.clock.now(),
            )
            .unwrap();

        let vote = ReceivedVote::new(
            Vote::new(&rep_key, UnixMillisTimestamp::new(123), 0, vec![block_hash]).into(),
            VoteSource::Live,
            None,
        );

        let results = service.apply_vote(&vote.clone().into());
        assert_eq!(results.get(&block_hash), Some(&Ok(())));

        let receiver_guard = service.events_rx.lock().unwrap();
        let receiver = receiver_guard.as_ref().unwrap();
        let mut observed_vote_processed = false;
        let start = std::time::Instant::now();

        while start.elapsed() < Duration::from_secs(5) {
            match receiver.try_recv() {
                Ok(AecEvent::VoteProcessed(processed_vote, voter_weight, per_block_results)) => {
                    assert_eq!(processed_vote.vote.hashes, vote.vote.hashes);
                    assert_eq!(voter_weight, Amount::MAX);
                    assert_eq!(per_block_results.get(&block_hash), Some(&Ok(())));
                    observed_vote_processed = true;
                    break;
                }
                Ok(
                    AecEvent::ElectionStarted(_, _)
                    | AecEvent::ElectionConfirmed(_)
                    | AecEvent::ElectionEnded(_),
                ) => {}
                Ok(other) => panic!("unexpected event: {:?}", std::mem::discriminant(&other)),
                Err(TryRecvError::Empty) => std::thread::yield_now(),
                Err(TryRecvError::Disconnected) => break,
            }
        }

        assert!(observed_vote_processed);
    }

    #[derive(Clone, Default)]
    struct StubProcessor {
        log: Arc<Mutex<Vec<&'static str>>>,
    }

    impl StubProcessor {
        fn log(&self) -> Vec<&'static str> {
            self.log.lock().unwrap().clone()
        }
    }

    impl BackpressureEventProcessor<AecEvent> for StubProcessor {
        fn cool_down(&mut self) {}

        fn recovered(&mut self) {
            self.log.lock().unwrap().push("recovered");
        }

        fn process(&mut self, _event: AecEvent) {
            self.log.lock().unwrap().push("processed");
        }
    }
}
