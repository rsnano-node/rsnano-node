mod account_priority_tracker;

mod block_handoff_queue;
mod blocked;
mod download_queue;
mod downloading;
mod logic;
mod priority;
mod stats;

pub use account_priority_tracker::{PriorityDownResult, PriorityUpResult};
pub use logic::{
    BootstrapQueueConfig, BootstrapQueueInfo, BootstrapQueueSnapshot, BootstrappingAccountInfo,
};
pub use priority::Priority;

use logic::BootstrapQueueLogic;

use std::{
    collections::VecDeque,
    sync::{atomic::Ordering::Relaxed, Mutex},
};

use rsnano_nullable_clock::SteadyClock;
use rsnano_types::{Account, Block, BlockHash};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{StatsCollection, StatsSource},
};

use crate::bootstrap::bootstrapper::bootstrap_queue::logic::TrimCount;
use stats::BootstrapQueueStats;

pub(crate) struct BootstrapQueue {
    logic: Mutex<BootstrapQueueLogic>,
    clock: SteadyClock,
    stats: BootstrapQueueStats,
}

impl BootstrapQueue {
    pub fn new(config: BootstrapQueueConfig) -> Self {
        Self::new_impl(config, SteadyClock::default())
    }

    #[cfg(test)]
    pub fn new_null() -> Self {
        Self::new_impl(Default::default(), SteadyClock::new_null())
    }

    fn new_impl(config: BootstrapQueueConfig, clock: SteadyClock) -> Self {
        Self {
            logic: Mutex::new(BootstrapQueueLogic::new(config)),
            clock,
            stats: Default::default(),
        }
    }

    pub fn enqueue(&self, account: Account) {
        let inserted;
        let mut trim_count = TrimCount::default();
        {
            let mut logic = self.logic.lock().unwrap();
            inserted = logic.enqueue(account);
            if inserted {
                trim_count = logic.trim_overflow();
            }
        }
        if inserted {
            self.stats.inserted.fetch_add(1, Relaxed);
            self.stats.add_trim_count(&trim_count);
        }
    }

    pub fn priority_up(&self, account: &Account) {
        let prio_result = self.logic.lock().unwrap().priority_up(account);
        self.stats.add_prio_set_result(&prio_result);
    }

    pub fn priority_down(&self, account: &Account) {
        let result = self.logic.lock().unwrap().priority_down(account);
        self.stats.add_prio_down_result(&result);
    }

    #[cfg(test)]
    pub fn priority(&self, account: &Account) -> Priority {
        self.logic.lock().unwrap().priority(account)
    }

    pub fn block(&self, block_hash: &BlockHash, dependency: BlockHash) {
        let now = self.clock.now();
        let blocked;
        let trim_count;
        {
            let mut logic = self.logic.lock().unwrap();
            blocked = logic.block(block_hash, dependency, now);
            trim_count = logic.trim_overflow();
        }
        if blocked {
            self.stats.blocked.fetch_add(1, Relaxed);
        } else {
            self.stats.block_failed.fetch_add(1, Relaxed);
        }
        self.stats.add_trim_count(&trim_count);
    }

    #[cfg(test)]
    pub fn blocked(&self, account: &Account) -> bool {
        self.logic.lock().unwrap().blocked(account)
    }

    pub fn unblock(&self, account: Account) {
        let unblocked;
        let trim_count;
        {
            let mut logic = self.logic.lock().unwrap();
            unblocked = logic.unblock(account);
            trim_count = logic.trim_overflow();
        }
        if unblocked {
            self.stats.unblocked.fetch_add(1, Relaxed);
        }
        self.stats.add_trim_count(&trim_count);
    }

    pub fn dependency_account_requested(&self, dependency: &BlockHash) {
        let now = self.clock.now();
        self.logic
            .lock()
            .unwrap()
            .dependency_account_requested(dependency, now);
        self.stats.dependency_requested.fetch_add(1, Relaxed);
    }

    pub fn dependency_account_not_found(&self, dependency: &BlockHash) {
        self.logic
            .lock()
            .unwrap()
            .dependency_account_not_found(dependency);
    }

    /// Sets information about the account chain that contains the block hash
    pub fn dependency_update(&self, dependency: &BlockHash, dependency_account: Account) {
        let mut logic = self.logic.lock().unwrap();
        let (updated, prio_result) = logic.dependency_update(dependency, dependency_account);
        if updated > 0 {
            self.stats
                .dependency_update
                .fetch_add(updated as u64, Relaxed);
            self.stats.add_prio_set_result(&prio_result);
        }
    }

    pub fn next_download_target(&self) -> Option<(Account, Priority)> {
        self.logic.lock().unwrap().next_download_target()
    }

    pub fn take_next_block_for_processing(&self) -> Option<Block> {
        let block = self.logic.lock().unwrap().take_next_block_for_processing();
        if block.is_some() {
            self.stats.processing_started.fetch_add(1, Relaxed);
        }
        block
    }

    pub fn revert_processing_started(&self, block_hash: &BlockHash) {
        let reverted = self
            .logic
            .lock()
            .unwrap()
            .revert_processing_started(block_hash);
        if reverted {
            self.stats.processing_started_reverted.fetch_add(1, Relaxed);
        }
    }

    pub fn download_started(&self, account: &Account) {
        let now = self.clock.now();
        let started = self.logic.lock().unwrap().download_started(account, now);
        if started {
            self.stats.download_started.fetch_add(1, Relaxed);
        } else {
            self.stats.download_start_failed.fetch_add(1, Relaxed);
        }
    }

    pub fn download_finished(&self, account: &Account, blocks: VecDeque<Block>) {
        let finished = self
            .logic
            .lock()
            .unwrap()
            .download_finished(account, blocks);
        if finished {
            self.stats.download_finished.fetch_add(1, Relaxed);
        } else {
            self.stats.download_finished_failed.fetch_add(1, Relaxed);
        }
    }

    pub fn remove_from_download_queue(&self, account: &Account) {
        self.logic
            .lock()
            .unwrap()
            .remove_from_download_queue(account);
    }

    pub fn processing_finished(&self, block_hash: &BlockHash) {
        let finished = self.logic.lock().unwrap().processing_finished(block_hash);
        if finished {
            self.stats.processing_finished.fetch_add(1, Relaxed);
        } else {
            self.stats.processing_finished_failed.fetch_add(1, Relaxed);
        }
    }

    pub fn processing_failed(&self, block_hash: &BlockHash) {
        self.logic.lock().unwrap().processing_failed(block_hash);
    }

    pub fn reprocess(&self, block_hash: &BlockHash) {
        let reprocessed = self.logic.lock().unwrap().reprocess(block_hash);
        if reprocessed {
            self.stats.reprocess.fetch_add(1, Relaxed);
        } else {
            self.stats.reprocess_failed.fetch_add(1, Relaxed);
        }
    }

    pub fn next_unknown_blocking_hash(&self) -> Option<BlockHash> {
        self.logic.lock().unwrap().next_unknown_blocking_hash()
    }

    /// Sets information about the account chain that contains the block hash
    /// Returns the number of inserted accounts
    pub fn sync_dependencies(&self) -> usize {
        self.logic.lock().unwrap().sync_dependencies()
    }

    pub fn contains(&self, account: &Account) -> bool {
        self.logic.lock().unwrap().contains(account)
    }

    pub fn snapshot(&self, limit: usize, filter: Option<Account>) -> BootstrapQueueSnapshot {
        self.logic.lock().unwrap().snapshot(limit, filter)
    }

    pub fn info(&self) -> BootstrapQueueInfo {
        self.logic.lock().unwrap().info()
    }

    pub fn queue_half_full(&self) -> bool {
        self.logic.lock().unwrap().queue_half_full()
    }

    pub fn blocked_half_full(&self) -> bool {
        self.logic.lock().unwrap().blocked_half_full()
    }

    pub fn clear_blocked_accounts(&self) {
        self.logic.lock().unwrap().clear_blocked_accounts();
    }

    pub fn missing_sends(&self) -> Vec<BlockHash> {
        self.logic.lock().unwrap().missing_sends()
    }

    pub fn processing(&self) -> Vec<BlockHash> {
        self.logic.lock().unwrap().processing()
    }

    pub fn timeout(&self) {
        let now = self.clock.now();
        let decayed;
        let trim_count;
        {
            let mut logic = self.logic.lock().unwrap();
            decayed = logic.timeout(now);
            trim_count = logic.trim_overflow();
        }
        self.stats
            .decayed_blocked
            .fetch_add(decayed as u64, Relaxed);
        self.stats.add_trim_count(&trim_count);
    }

    pub fn revision(&self) -> u64 {
        self.logic.lock().unwrap().revision()
    }
}

impl Default for BootstrapQueue {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl ContainerInfoProvider for BootstrapQueue {
    fn container_info(&self) -> ContainerInfo {
        self.logic.lock().unwrap().container_info()
    }
}

impl StatsSource for BootstrapQueue {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.stats.collect_stats(result);
    }
}
