use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
};

use rsnano_messages::BlocksAckPayload;
use rsnano_utils::stats::{StatsCollection, StatsSource};

use crate::bootstrap::bootstrapper::{
    VerifyResult,
    bootstrap_queue::{BootstrapQueue, Priority},
    query_tracker::{QueryType, RunningQuery},
};

pub(crate) struct BlockAckProcessor {
    bootstrap_queue: Arc<BootstrapQueue>,
    stats: BlockAckStats,
}

impl BlockAckProcessor {
    pub fn new(bootstrap_queue: Arc<BootstrapQueue>) -> Self {
        Self {
            bootstrap_queue,
            stats: Default::default(),
        }
    }

    pub(crate) fn process(&self, query: &RunningQuery, response: BlocksAckPayload) -> bool {
        let result = query.verify_blocks(&response);
        match result {
            VerifyResult::Ok => {
                self.process_valid_blocks(query, response);
                true
            }
            VerifyResult::NothingNew => {
                self.process_empty_response(query);
                true
            }
            VerifyResult::Invalid => {
                self.bootstrap_queue
                    .download_finished(&query.account, VecDeque::new());
                self.stats.invalid.fetch_add(1, Relaxed);
                false
            }
        }
    }

    fn process_valid_blocks(&self, query: &RunningQuery, response: BlocksAckPayload) {
        self.stats.verified.fetch_add(1, Relaxed);
        self.stats
            .blocks
            .fetch_add(response.blocks().len() as u64, Relaxed);

        let mut blocks = response.take_blocks();
        let is_end = blocks.len() < query.count;

        if query.query_type == QueryType::BlocksByHash {
            // Avoid re-processing the block we already have
            blocks.pop_front();
        }

        self.bootstrap_queue
            .download_finished(&query.account, blocks);
        if is_end {
            self.bootstrap_queue
                .remove_from_download_queue(&query.account)
        }
    }

    fn process_empty_response(&self, query: &RunningQuery) {
        self.stats.nothing_new.fetch_add(1, Relaxed);
        self.bootstrap_queue
            .download_finished(&query.account, VecDeque::new());
        self.bootstrap_queue
            .remove_from_download_queue(&query.account)
    }
}

impl StatsSource for BlockAckProcessor {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.stats.collect_stats(result);
    }
}

#[derive(Default)]
pub(crate) struct BlockAckStats {
    invalid: AtomicU64,
    verified: AtomicU64,
    blocks: AtomicU64,
    nothing_new: AtomicU64,
}

impl StatsSource for BlockAckStats {
    fn collect_stats(&self, result: &mut StatsCollection) {
        result.insert(
            "bootstrap_verify_blocks",
            "invalid",
            self.invalid.load(Relaxed),
        );
        result.insert("bootstrap_verify_blocks", "ok", self.verified.load(Relaxed));
        result.insert(
            "bootstrap_verify_blocks",
            "nothing_new",
            self.nothing_new.load(Relaxed),
        );
        result.insert("bootstrap", "blocks", self.blocks.load(Relaxed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::bootstrapper::query_tracker::QueryType;
    use rsnano_types::Account;

    #[test]
    fn response_doesnt_match_query() {
        let queue = Arc::new(BootstrapQueue::new_null());
        let processor = BlockAckProcessor::new(queue.clone());

        let query = RunningQuery::new_test_instance();
        let response = BlocksAckPayload::new_test_instance();
        let ok = processor.process(&query, response);
        assert!(!ok);
        assert_eq!(processor.stats.invalid.load(Relaxed), 1);
    }

    #[test]
    fn handle_empty_response() {
        let queue = Arc::new(BootstrapQueue::default());
        let processor = BlockAckProcessor::new(queue.clone());
        let account = Account::from(42);

        let query = RunningQuery {
            query_type: QueryType::BlocksByAccount,
            account,
            ..RunningQuery::new_test_instance()
        };

        let response = BlocksAckPayload::empty();
        let ok = processor.process(&query, response);

        assert!(ok);
        assert_eq!(processor.stats.nothing_new.load(Relaxed), 1);
    }
}
