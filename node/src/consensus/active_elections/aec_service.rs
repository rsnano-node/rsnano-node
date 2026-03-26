use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use rsnano_types::{BlockHash, QualifiedRoot};
use rsnano_utils::sync::backpressure_channel::{self, Receiver, Sender};

use crate::{
    consensus::{
        ActiveElectionsConfig, ActiveElectionsContainer, AecCooldownReason, AecEvent, VoteProcessor,
    },
    utils::{BackpressureEventProcessor, spawn_backpressure_processor},
};

pub(crate) struct AecService {
    active: Arc<RwLock<ActiveElectionsContainer>>,
    events_tx: Sender<AecEvent>,
    events_rx: Mutex<Option<Receiver<AecEvent>>>,
}

impl AecService {
    const EVENT_QUEUE_SOFT_LIMIT: usize = 1024 * 5;

    pub(crate) fn new(config: ActiveElectionsConfig, base_latency: Duration) -> Self {
        let (events_tx, events_rx) = backpressure_channel::channel(Self::EVENT_QUEUE_SOFT_LIMIT);

        let mut active = ActiveElectionsContainer::new(config, base_latency);
        active.set_observer(events_tx.clone());

        Self {
            active: Arc::new(RwLock::new(active)),
            events_tx,
            events_rx: Mutex::new(Some(events_rx)),
        }
    }

    pub(crate) fn new_null() -> Self {
        Self::new(ActiveElectionsConfig::default(), Duration::from_secs(1))
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

    pub(crate) fn observe_vote_processor(&self, vote_processor: &VoteProcessor) {
        vote_processor.add_observer(self.events_tx.clone());
    }

    pub(crate) fn set_cooldown(&self, cool_down: bool, reason: AecCooldownReason) {
        self.active.write().unwrap().set_cooldown(cool_down, reason);
    }

    pub(crate) fn erase(&self, root: &QualifiedRoot) -> bool {
        self.active.write().unwrap().erase(root)
    }

    pub(crate) fn remove_recently_confirmed(&self, block_hash: &BlockHash) {
        self.active
            .write()
            .unwrap()
            .remove_recently_confirmed(block_hash);
    }

    /// Temporary compatibility bridge for collaborators that have not yet migrated to AecService.
    pub(crate) fn legacy_container(&self) -> Arc<RwLock<ActiveElectionsContainer>> {
        Arc::clone(&self.active)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::consensus::AecEvent;

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
