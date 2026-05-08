use std::sync::atomic::Ordering::Relaxed;

use rsnano_ledger::OwningAnySet;
use rsnano_types::Frontier;
use rsnano_utils::stats::{DetailType, StatType, Stats};

use super::{frontier_checker::FrontierChecker, logic::OutdatedAccounts, stats::FrontierScanStats};
use crate::bootstrap::bootstrapper::bootstrap_queue::{BootstrapQueue, Priority};

/// Handles received frontiers
pub(crate) struct FrontierWorker<'a> {
    stats: &'a Stats,
    stats2: &'a FrontierScanStats,
    checker: FrontierChecker<'a>,
    bootstrap_queue: &'a BootstrapQueue,
}

impl<'a> FrontierWorker<'a> {
    pub(crate) fn new(
        any: &'a OwningAnySet<'a>,
        stats: &'a Stats,
        stats2: &'a FrontierScanStats,
        bootstrap_queue: &'a BootstrapQueue,
    ) -> Self {
        Self {
            stats,
            stats2,
            checker: FrontierChecker::new(any),
            bootstrap_queue,
        }
    }

    pub fn process(&mut self, frontiers: Vec<Frontier>) {
        let outdated = self.checker.get_outdated_accounts(&frontiers);
        self.update_stats(&frontiers, &outdated);
        self.stats2
            .processed_frontiers
            .fetch_add(outdated.frontiers_received as u64, Relaxed);
        self.stats2
            .outdated_accounts_found
            .fetch_add(outdated.accounts.len() as u64, Relaxed);
        self.stats2.add(&outdated);

        for account in &outdated.accounts {
            self.bootstrap_queue.enqueue(*account);
        }
    }

    fn update_stats(&self, frontiers: &[Frontier], outdated: &OutdatedAccounts) {
        self.stats.add(
            StatType::BootstrapFrontiers,
            DetailType::Processed,
            frontiers.len() as u64,
        );
        self.stats.add(
            StatType::BootstrapFrontiers,
            DetailType::Prioritized,
            outdated.accounts.len() as u64,
        );
        self.stats.add(
            StatType::BootstrapFrontiers,
            DetailType::Outdated,
            outdated.outdated as u64,
        );
        self.stats.add(
            StatType::BootstrapFrontiers,
            DetailType::Pending,
            outdated.pending as u64,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::bootstrapper::{Priority, frontier_scan::stats::FrontierScanStats};
    use rsnano_ledger::Ledger;
    use rsnano_types::{Account, AccountInfo, BlockHash};
    use std::sync::Arc;

    #[test]
    fn empty() {
        let ledger = Ledger::new_null();
        let any = ledger.any();
        let stats = Stats::default();
        let bootstrap_queue = Arc::new(BootstrapQueue::new_null());
        let stats2 = Arc::new(FrontierScanStats::default());
        let mut worker = FrontierWorker::new(&any, &stats, &stats2, &bootstrap_queue);

        worker.process(Vec::new());

        assert_eq!(bootstrap_queue.info().download_queue, 0);
    }

    #[test]
    fn prioritize_one_account() {
        let account = Account::from(1);
        let ledger = Ledger::new_null_builder()
            .account_info(
                &account,
                &AccountInfo {
                    head: BlockHash::from(2),
                    ..Default::default()
                },
            )
            .finish();
        let any = ledger.any();
        let stats = Stats::default();
        let bootstrap_queue = Arc::new(BootstrapQueue::new_null());
        let stats2 = Arc::new(FrontierScanStats::default());
        let mut worker = FrontierWorker::new(&any, &stats, &stats2, &bootstrap_queue);

        worker.process(vec![Frontier::new(account, BlockHash::from(3))]);

        assert_eq!(bootstrap_queue.info().download_queue, 1);
        assert_eq!(bootstrap_queue.priority(&account), Priority::INITIAL);
        assert_eq!(stats2.outdated_accounts_found.load(Relaxed), 1);
        assert_eq!(stats2.processed_frontiers.load(Relaxed), 1);
    }
}
