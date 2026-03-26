use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use rsnano_ledger::RepWeightCache;
use rsnano_nullable_clock::{SteadyClock, Timestamp};
use rsnano_types::{
    Amount, Block, BlockHash, BlockPriority, PublicKey, QualifiedRoot, Root, SavedBlock, VoteError,
};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{StatsCollection, StatsSource},
    sync::backpressure_channel::{self, Receiver, Sender},
};

use super::AecFacts;

use crate::{
    consensus::{
        ActiveElectionsConfig, ActiveElectionsContainer, AecCooldownReason, AecEvent,
        AecInsertError, AecInsertRequest, AecSchedulerRequest, AecTickerRead, ApplyVoteArgs,
        FilteredVote, ReceivedVote,
        election::{ConfirmedElection, Election, ElectionBehavior, ElectionState, VoteType},
        election_schedulers::priority::PriorityBucketState,
    },
    representatives::OnlineReps,
    utils::{BackpressureEventProcessor, spawn_backpressure_processor},
};

pub struct AecService {
    active: Arc<RwLock<ActiveElectionsContainer>>,
    events_tx: Mutex<Option<Sender<AecEvent>>>,
    events_rx: Mutex<Option<Receiver<AecEvent>>>,
    online_reps: Arc<Mutex<OnlineReps>>,
    clock: Arc<SteadyClock>,
    rep_weights: Arc<RepWeightCache>,
    is_dev_network: bool,
}

impl AecService {
    const EVENT_QUEUE_SOFT_LIMIT: usize = 1024 * 5;

    pub fn new(
        config: ActiveElectionsConfig,
        base_latency: Duration,
        online_reps: Arc<Mutex<OnlineReps>>,
        clock: Arc<SteadyClock>,
        rep_weights: Arc<RepWeightCache>,
        is_dev_network: bool,
    ) -> Self {
        let (events_tx, events_rx) = backpressure_channel::channel(Self::EVENT_QUEUE_SOFT_LIMIT);

        Self {
            active: Arc::new(RwLock::new(ActiveElectionsContainer::new(
                config,
                base_latency,
            ))),
            events_tx: Mutex::new(Some(events_tx)),
            events_rx: Mutex::new(Some(events_rx)),
            online_reps,
            clock,
            rep_weights,
            is_dev_network,
        }
    }

    pub fn new_null() -> Self {
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
        self.events_tx
            .lock()
            .unwrap()
            .as_ref()
            .map(|sender| sender.len())
            .unwrap_or(0)
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
        let facts = self.active.write().unwrap().set_cooldown(cool_down, reason);
        self.publish_facts(facts);
    }

    pub fn erase(&self, root: &QualifiedRoot) -> bool {
        let facts = self.active.write().unwrap().erase(root);
        let erased = facts.is_some();
        if let Some(facts) = facts {
            self.publish_facts(facts);
        }
        erased
    }

    pub fn max_len(&self) -> usize {
        self.active.read().unwrap().max_len()
    }

    // Shared AEC query surface used by production callers and tests.
    // These methods answer AEC-shaped questions without exposing caller-specific
    // traversal or runtime helpers.
    pub fn vacancy(&self) -> i64 {
        self.active.read().unwrap().vacancy()
    }

    pub fn info(&self) -> crate::consensus::ActiveElectionsInfo {
        self.active.read().unwrap().info()
    }

    pub fn was_recently_confirmed(&self, block_hash: &BlockHash) -> bool {
        self.active
            .read()
            .unwrap()
            .was_recently_confirmed(block_hash)
    }

    pub fn elections_round_robin(&self) -> Vec<Election> {
        self.active
            .read()
            .unwrap()
            .iter_round_robin()
            .cloned()
            .collect()
    }

    pub fn count_by_behavior(&self, behavior: ElectionBehavior) -> usize {
        self.active.read().unwrap().count_by_behavior(behavior)
    }

    pub fn is_active_root(&self, root: &QualifiedRoot) -> bool {
        self.active.read().unwrap().is_active_root(root)
    }

    pub fn is_active_hash(&self, hash: &BlockHash) -> bool {
        self.active.read().unwrap().is_active_hash(hash)
    }

    pub fn election_for_root(&self, root: &QualifiedRoot) -> Option<Election> {
        self.active.read().unwrap().election_for_root(root).cloned()
    }

    pub fn election_for_block(&self, hash: &BlockHash) -> Option<Election> {
        self.active
            .read()
            .unwrap()
            .election_for_block(hash)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.active.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.read().unwrap().is_empty()
    }

    pub(crate) fn remove_recently_confirmed(&self, block_hash: &BlockHash) {
        self.active
            .write()
            .unwrap()
            .remove_recently_confirmed(block_hash);
    }

    // Shared AEC mutation surface. Caller-specific helpers below are temporary and
    // are removed unit by unit as the boundary is narrowed.
    pub(crate) fn confirm_dependent_elections(
        &self,
        confirmed: Vec<(SavedBlock, Option<ConfirmedElection>)>,
    ) {
        let facts = self
            .active
            .write()
            .unwrap()
            .confirm_dependent_elections(confirmed, self.clock.now());
        self.publish_facts(facts);
    }

    pub(crate) fn try_add_fork(&self, fork: &Block, fork_tally: Amount) -> bool {
        let (added, facts) = self.active.write().unwrap().try_add_fork(fork, fork_tally);
        self.publish_facts(facts);
        added
    }

    pub(crate) fn transition_time(&self) {
        let facts = self
            .active
            .write()
            .unwrap()
            .transition_time(self.clock.now());
        self.publish_facts(facts);
    }

    pub fn transition_active(&self, block_hash: &BlockHash) -> bool {
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

    pub(crate) fn scheduler_activate(
        &self,
        request: AecSchedulerRequest,
    ) -> Result<(), AecInsertError> {
        let hash = request.block_hash();
        let transition_active = request.transitions_to_active();
        self.insert_impl(request.into())?;
        if transition_active {
            self.transition_active(&hash);
        }
        Ok(())
    }

    fn insert_impl(&self, request: AecInsertRequest) -> Result<(), AecInsertError> {
        let facts = self
            .active
            .write()
            .unwrap()
            .insert(request, self.clock.now())?;
        self.publish_facts(facts);
        Ok(())
    }

    pub fn insert_priority(
        &self,
        block: SavedBlock,
        priority: BlockPriority,
    ) -> Result<(), AecInsertError> {
        self.insert_impl(AecInsertRequest::new_priority(block, priority))
    }

    pub(crate) fn priority_bucket_state(
        &self,
        bucket: usize,
        candidate_root: &QualifiedRoot,
    ) -> PriorityBucketState {
        self.active
            .read()
            .unwrap()
            .priority_bucket_state(bucket, candidate_root)
    }

    pub(crate) fn next_vote_to_broadcast(
        &self,
        bucket: usize,
        vote_broadcast_interval: Duration,
        now: Timestamp,
    ) -> Option<(Root, BlockHash, VoteType)> {
        let mut active = self.active.write().unwrap();
        let vote_target = active.iter_bucket(bucket).find_map(|election| {
            if election.can_vote(vote_broadcast_interval, now) {
                Some((
                    election.qualified_root().clone(),
                    election.vote_type(),
                    election.winner().hash(),
                ))
            } else {
                None
            }
        });

        vote_target.map(|(qualified_root, vote_type, winner_hash)| {
            active.set_last_voted(&qualified_root, vote_type, now);
            (qualified_root.root, winner_hash, vote_type)
        })
    }

    pub fn clear_recently_confirmed(&self) {
        self.active.write().unwrap().clear_recently_confirmed();
    }

    pub fn force_confirm(&self, block_hash: &BlockHash) {
        let facts = self
            .active
            .write()
            .unwrap()
            .force_confirm(block_hash, self.clock.now());
        self.publish_facts(facts);
    }

    pub fn cancel(&self, root: &QualifiedRoot) {
        self.active.write().unwrap().cancel(root);
    }

    pub fn cancel_all(&self) {
        self.active.write().unwrap().cancel_all();
    }

    pub fn simulate_event(&self, event: AecEvent) {
        self.send_event(event);
    }

    pub fn stop(&self) {
        self.events_tx.lock().unwrap().take();
        self.active.write().unwrap().stop();
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

        let per_block = results.per_block;
        self.publish_facts(results.facts);
        self.notify_vote_processed(vote.vote.clone(), voter_weight, &per_block);
        per_block
    }

    fn publish_facts(&self, facts: AecFacts) {
        for fact in facts {
            self.send_event(fact.into());
        }
    }

    fn notify_vote_processed(
        &self,
        vote: ReceivedVote,
        voter_weight: Amount,
        results: &HashMap<BlockHash, Result<(), VoteError>>,
    ) {
        self.send_event(AecEvent::VoteProcessed(vote, voter_weight, results.clone()));
    }

    fn send_event(&self, event: AecEvent) {
        if let Some(sender) = self.events_tx.lock().unwrap().as_ref() {
            let _ = sender.send(event);
        }
    }

    #[cfg(test)]
    pub(crate) fn insert_for_test(
        &self,
        request: AecInsertRequest,
        now: Timestamp,
    ) -> Result<(), AecInsertError> {
        let facts = self.active.write().unwrap().insert(request, now)?;
        self.publish_facts(facts);
        Ok(())
    }
}

impl StatsSource for AecService {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.active.read().unwrap().collect_stats(result);
    }
}

impl ContainerInfoProvider for AecService {
    fn container_info(&self) -> ContainerInfo {
        self.active.read().unwrap().container_info()
    }
}

impl AecTickerRead for AecService {
    fn for_each_confirmation_solicitation_election(&self, action: &mut dyn FnMut(&Election)) {
        let active = self.active.read().unwrap();
        for election in active.iter_round_robin() {
            if election.state() == ElectionState::Active {
                action(election);
            }
        }
    }

    fn for_each_stale_election(
        &self,
        now: Timestamp,
        stale_threshold: Duration,
        action: &mut dyn FnMut(&Election),
    ) {
        let active = self.active.read().unwrap();
        for election in active.iter_round_robin() {
            if election.start().elapsed(now) >= stale_threshold {
                action(election);
            }
        }
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

        service.simulate_event(AecEvent::Recovered);

        assert_eq!(service.event_queue_len(), 1);
    }

    #[test]
    fn processing_starts_only_when_explicitly_requested() {
        let service = AecService::new_null();
        let processor = StubProcessor::default();

        service.simulate_event(AecEvent::Recovered);
        assert!(processor.log().is_empty());

        service.start_event_processor("aec-service-test", processor.clone());

        let start = std::time::Instant::now();
        while processor.log().is_empty() && start.elapsed() < Duration::from_secs(5) {
            std::thread::yield_now();
        }

        assert_eq!(processor.log(), vec!["processed"]);
    }

    #[test]
    fn stop_closes_event_queue_and_rejects_further_publication() {
        let service = AecService::new_null();

        service.stop();
        service.simulate_event(AecEvent::Recovered);

        assert_eq!(service.event_queue_len(), 0);
        assert!(matches!(
            service
                .events_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn apply_vote_publishes_vote_processed_event() {
        let service = AecService::new_null();
        let rep_key = PrivateKey::from(1);
        let block = SavedBlock::new_test_instance();
        let block_hash = block.hash();

        service.rep_weights.put(rep_key.public_key(), Amount::MAX);
        service
            .insert_for_test(
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
