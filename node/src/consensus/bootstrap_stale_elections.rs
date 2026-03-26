use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rsnano_nullable_clock::SteadyClock;
use rsnano_types::Account;
use rsnano_utils::stats::{StatsCollection, StatsSource};

use super::{AecTickerPlugin, AecTickerRead};
use crate::bootstrap::Bootstrapper;

/// If an election isn't confirmed within "stale_threshold", then try to bootstrap
/// the election account, so that missing dependencies will be pulled
pub(crate) struct BootstrapStaleElections {
    bootstrapper: Arc<Bootstrapper>,
    clock: Arc<SteadyClock>,
    pub stats: Arc<StaleElectionsStats>,
    stale_threshold: Duration,
    stale_accounts: Vec<Account>,
}

impl BootstrapStaleElections {
    pub const DEFAULT_STALE_THRESHOLD: Duration = Duration::from_secs(60);

    pub(crate) fn new(bootstrapper: Arc<Bootstrapper>, clock: Arc<SteadyClock>) -> Self {
        Self {
            bootstrapper,
            clock,
            stats: Arc::new(StaleElectionsStats::default()),
            stale_threshold: Self::DEFAULT_STALE_THRESHOLD,
            stale_accounts: Vec::new(),
        }
    }

    pub fn set_stale_threshold(&mut self, threshold: Duration) {
        self.stale_threshold = threshold;
    }

    #[allow(dead_code)]
    pub fn get_stale_threshold(&self) -> Duration {
        self.stale_threshold
    }

    fn bootstrap_stale_accounts(&mut self) {
        let mut state = self.bootstrapper.state();

        for account in &self.stale_accounts {
            state.candidate_accounts.priority_set_initial(account);
        }
        self.stats
            .bootstrap_stale
            .fetch_add(self.stale_accounts.len() as u64, Ordering::Relaxed);
    }
}

impl AecTickerPlugin for BootstrapStaleElections {
    fn run(&mut self, aec: &dyn AecTickerRead) {
        let now = self.clock.now();

        self.stale_accounts.clear();
        aec.for_each_stale_election(now, self.stale_threshold, &mut |election| {
            if self.stale_accounts.len() < 128 {
                self.stale_accounts.push(election.account());
            }
        });

        self.bootstrap_stale_accounts();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Default)]
pub(crate) struct StaleElectionsStats {
    pub bootstrap_stale: AtomicU64,
}

impl StatsSource for StaleElectionsStats {
    fn collect_stats(&self, result: &mut StatsCollection) {
        result.insert(
            "active_elections",
            "bootstrap_stale",
            self.bootstrap_stale.load(Ordering::Relaxed),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::{AecInsertRequest, AecService};
    use rsnano_types::{BlockPriority, SavedBlock};

    #[test]
    fn process_empty() {
        let bootstrapper = Arc::new(Bootstrapper::new_null());
        let clock = Arc::new(SteadyClock::new_null());
        let mut plugin = BootstrapStaleElections::new(bootstrapper.clone(), clock);
        let aec = AecService::new_null();

        plugin.run(&aec);

        assert_eq!(bootstrapper.state().candidate_accounts.priority_len(), 0);
        assert_eq!(plugin.stats.bootstrap_stale.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn bootstrap_stale_election() {
        let bootstrapper = Arc::new(Bootstrapper::new_null());
        let clock = Arc::new(SteadyClock::new_null());
        let block = SavedBlock::new_test_instance();
        let prio = BlockPriority::new_test_instance();
        let account = block.account();
        let aec = AecService::new_null();
        aec.insert_for_test(
            AecInsertRequest::new_priority(block, prio),
            clock.now() - BootstrapStaleElections::DEFAULT_STALE_THRESHOLD,
        )
        .unwrap();

        let mut plugin = BootstrapStaleElections::new(bootstrapper.clone(), clock);
        plugin.run(&aec);

        assert!(
            bootstrapper
                .state()
                .candidate_accounts
                .prioritized(&account)
        );
        assert_eq!(plugin.stats.bootstrap_stale.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stats() {
        let stats = StaleElectionsStats {
            bootstrap_stale: AtomicU64::new(123),
        };
        let mut collection = StatsCollection::default();
        stats.collect_stats(&mut collection);
        assert_eq!(collection.get("active_elections", "bootstrap_stale"), 123);
    }
}
