use crate::{
    block_processing::LedgerPipelineEvent, consensus::AecService, ledger_snapshots::LedgerSnapshots,
};
use rsnano_ledger::LedgerEvent;
use rsnano_ledger::{BlockError, Ledger};
use rsnano_utils::EventHandlerMut;
use std::sync::Arc;

pub(crate) struct ForkDetector {
    ledger: Arc<Ledger>,
    ledger_snapshots: Arc<LedgerSnapshots>,
    aec_service: Arc<AecService>,
}

impl ForkDetector {
    pub(crate) fn new(
        ledger: Arc<Ledger>,
        ledger_snapshots: Arc<LedgerSnapshots>,
        aec_service: Arc<AecService>,
    ) -> Self {
        Self {
            ledger,
            ledger_snapshots,
            aec_service,
        }
    }
}

impl EventHandlerMut<LedgerPipelineEvent> for ForkDetector {
    fn handle(&mut self, event: &LedgerPipelineEvent) {
        if let LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(results)) = event {
            for result in results {
                if result.status == Err(BlockError::Fork) {
                    let root = result.block.qualified_root();
                    tracing::debug!("Fork detected: {:?}", root);

                    self.ledger
                        .mark_fork(&root, self.ledger_snapshots.get_current_snapshot_number());

                    self.aec_service.erase(&root);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        block_processing::LedgerPipelineEvent,
        block_processing::{BlockSource, ProcessedResult},
        consensus::{AecInsertRequest, AecService, election::ElectionBehavior},
        ledger_snapshots::{LedgerSnapshots, fork_detector::ForkDetector},
    };
    use rsnano_ledger::LedgerEvent;
    use rsnano_ledger::{BlockError, Ledger};
    use rsnano_nullable_clock::Timestamp;
    use rsnano_types::{Block, BlockPriority, SavedBlock};
    use rsnano_utils::EventHandlerMut;
    use std::sync::Arc;

    #[test]
    fn marks_a_forked_block_in_the_ledger() {
        let ledger = Arc::new(Ledger::new_null());
        let ledger_snapshots = LedgerSnapshots::new_null();
        let snapshot_number = ledger_snapshots.get_current_snapshot_number();
        let mut fork_detector = ForkDetector::new(
            ledger.clone(),
            ledger_snapshots.into(),
            Arc::new(AecService::new_null()),
        );
        let block = Block::new_test_instance();
        let root = block.qualified_root();

        let processed_results = ProcessedResult {
            block,
            source: BlockSource::Live,
            status: Err(BlockError::Fork),
            saved_block: None,
        };

        fork_detector.handle(&LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(
            vec![processed_results],
        )));

        assert_eq!(
            ledger
                .store
                .forks
                .get(&ledger.store.env.begin_read(), &root),
            Some(snapshot_number)
        );
    }

    #[test]
    fn can_mark_multiple_forks_in_one_go() {
        let ledger = Arc::new(Ledger::new_null());
        let ledger_snapshots = LedgerSnapshots::new_null();
        let snapshot_number = ledger_snapshots.get_current_snapshot_number();
        let mut fork_detector = ForkDetector::new(
            ledger.clone(),
            ledger_snapshots.into(),
            Arc::new(AecService::new_null()),
        );
        let block1 = Block::new_test_instance_with_key(1);
        let block2 = Block::new_test_instance_with_key(2);
        let root1 = block1.qualified_root();
        let root2 = block2.qualified_root();

        let processed_result1 = ProcessedResult {
            block: block1,
            source: BlockSource::Live,
            status: Err(BlockError::Fork),
            saved_block: None,
        };

        let processed_result2 = ProcessedResult {
            block: block2,
            source: BlockSource::Live,
            status: Err(BlockError::Fork),
            saved_block: None,
        };

        fork_detector.handle(&LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(
            vec![processed_result1, processed_result2],
        )));

        assert_eq!(
            ledger
                .store
                .forks
                .get(&ledger.store.env.begin_read(), &root1),
            Some(snapshot_number)
        );

        assert_eq!(
            ledger
                .store
                .forks
                .get(&ledger.store.env.begin_read(), &root2),
            Some(snapshot_number)
        );
    }

    #[test]
    fn ignores_blocks_without_fork() {
        let ledger = Arc::new(Ledger::new_null());
        let ledger_snapshots = LedgerSnapshots::new_null();
        let mut fork_detector = ForkDetector::new(
            ledger.clone(),
            ledger_snapshots.into(),
            Arc::new(AecService::new_null()),
        );
        let block = Block::new_test_instance();
        let root = block.qualified_root();

        let processed_results = ProcessedResult {
            block,
            source: BlockSource::Live,
            status: Err(BlockError::GapPrevious),
            saved_block: None,
        };

        fork_detector.handle(&LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(
            vec![processed_results],
        )));

        assert_eq!(
            ledger
                .store
                .forks
                .get(&ledger.store.env.begin_read(), &root),
            None
        );
    }

    #[test]
    fn stop_forked_election() {
        let block = SavedBlock::new_test_instance();
        let aec_service = Arc::new(AecService::new_null());
        aec_service
            .insert_priority(block.clone(), BlockPriority::new_test_instance())
            .unwrap();

        let ledger = Arc::new(Ledger::new_null());
        let ledger_snapshots = LedgerSnapshots::new_null();
        let mut fork_detector =
            ForkDetector::new(ledger.clone(), ledger_snapshots.into(), aec_service.clone());

        let processed_results = ProcessedResult {
            block: block.into(),
            source: BlockSource::Live,
            status: Err(BlockError::Fork),
            saved_block: None,
        };

        fork_detector.handle(&LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(
            vec![processed_results],
        )));

        assert_eq!(
            fork_detector
                .aec_service
                .legacy_container()
                .read()
                .unwrap()
                .len(),
            0
        );
    }
}
