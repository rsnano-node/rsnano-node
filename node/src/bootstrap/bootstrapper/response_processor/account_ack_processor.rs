use rsnano_messages::AccountInfoAckPayload;
use rsnano_utils::stats::{StatsCollection, StatsSource};

use crate::bootstrap::bootstrapper::{
    bootstrap_queue::BootstrapQueue, query_tracker::RunningQuery,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub(crate) struct AccountAckProcessor {
    bootstrap_queue: Arc<BootstrapQueue>,
    stats: AccountAckStats,
}

impl AccountAckProcessor {
    pub fn new(bootstrap_queue: Arc<BootstrapQueue>) -> Self {
        Self {
            bootstrap_queue,
            stats: Default::default(),
        }
    }

    pub fn process(&self, query: &RunningQuery, response: &AccountInfoAckPayload) -> bool {
        if response.account.is_zero() {
            self.bootstrap_queue
                .dependency_account_not_found(&query.hash);
            self.stats.empty.fetch_add(1, Ordering::Relaxed);
            // OK, but nothing to do
            return true;
        }

        // Prioritize account containing the dependency
        self.bootstrap_queue
            .dependency_update(&query.hash, response.account);
        // OK, no way to verify the response
        true
    }
}

impl StatsSource for AccountAckProcessor {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.stats.collect_stats(result);
    }
}

#[derive(Default)]
pub(super) struct AccountAckStats {
    pub empty: AtomicU64,
}

impl StatsSource for AccountAckStats {
    fn collect_stats(&self, result: &mut StatsCollection) {
        const PROCESSOR: &str = "bootstr_acc_ack_proc";
        result.insert(
            PROCESSOR,
            "account_info_empty",
            self.empty.load(Ordering::Relaxed),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::bootstrapper::Priority;
    use rsnano_types::{Account, Block, BlockHash, PrivateKey, StateBlockArgs};

    #[test]
    fn empty_response() {
        let queue = Arc::new(BootstrapQueue::new_null());
        let processor = AccountAckProcessor::new(queue.clone());
        let query = RunningQuery::new_test_instance();

        let response = AccountInfoAckPayload {
            account: Account::ZERO,
            ..AccountInfoAckPayload::new_test_instance()
        };

        assert!(processor.process(&query, &response));

        assert_eq!(processor.stats.empty.load(Ordering::Relaxed), 1);
        assert_eq!(queue.info().download_queue, 0);
    }

    #[test]
    fn update_dependency() {
        let queue = Arc::new(BootstrapQueue::new_null());
        let processor = AccountAckProcessor::new(queue.clone());
        let key = PrivateKey::from(42);
        let blocked_account = key.account();
        let unknown_source = BlockHash::from(100);
        let source_account = Account::from(200);
        let receive: Block = StateBlockArgs {
            key: &key,
            link: unknown_source.into(),
            ..StateBlockArgs::new_test_instance()
        }
        .into();

        let query = RunningQuery {
            hash: unknown_source,
            ..RunningQuery::new_test_instance()
        };

        let response = AccountInfoAckPayload {
            account: source_account,
            ..AccountInfoAckPayload::new_test_instance()
        };

        queue.enqueue(blocked_account);
        queue.download_started(&blocked_account);
        queue.download_finished(&blocked_account, [receive].into());
        let next = queue.take_next_block_for_processing().unwrap();

        queue.block(&next.hash(), unknown_source);

        assert!(processor.process(&query, &response));

        assert!(queue.blocked(&blocked_account));
        assert!(queue.contains(&source_account));
        let (target, _) = queue.next_download_target().unwrap();
        assert_eq!(target, source_account);
    }
}
