use super::{PriorityDownResult, PriorityUpResult};
use crate::bootstrap::bootstrapper::bootstrap_queue::logic::TrimCount;
use rsnano_utils::stats::{StatsCollection, StatsSource};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub(crate) struct BootstrapQueueStats {
    pub inserted: AtomicU64,
    pub upgraded: AtomicU64,
    pub removed: AtomicU64,
    pub remove_failed: AtomicU64,
    pub deprioritized: AtomicU64,
    pub not_found: AtomicU64,
    pub blocked: AtomicU64,
    pub block_failed: AtomicU64,
    pub unblocked: AtomicU64,
    pub decayed_blocked: AtomicU64,
    pub dependency_update: AtomicU64,
    pub download_started: AtomicU64,
    pub download_start_failed: AtomicU64,
    pub download_finished: AtomicU64,
    pub download_finished_failed: AtomicU64,
    pub processing_started: AtomicU64,
    pub processing_started_reverted: AtomicU64,
    pub reprocess: AtomicU64,
    pub reprocess_failed: AtomicU64,
    pub processing_finished: AtomicU64,
    pub processing_finished_failed: AtomicU64,
    pub dependency_requested: AtomicU64,
    pub trim_download_queue: AtomicU64,
    pub trim_blocked: AtomicU64,
}

impl BootstrapQueueStats {
    pub fn add_prio_set_result(&self, result: &PriorityUpResult) {
        match result {
            PriorityUpResult::NotFound => self.not_found.fetch_add(1, Relaxed),
            PriorityUpResult::Upgraded(_, _) => self.upgraded.fetch_add(1, Relaxed),
            PriorityUpResult::Unchanged => 0,
        };
    }

    pub fn add_prio_down_result(&self, result: &PriorityDownResult) {
        match result {
            PriorityDownResult::Deprioritized(_, _) => self.deprioritized.fetch_add(1, Relaxed),
            PriorityDownResult::Removed => self.removed.fetch_add(1, Relaxed),
            PriorityDownResult::AccountNotFound => self.not_found.fetch_add(1, Relaxed),
        };
    }

    pub fn add_trim_count(&self, trim_count: &TrimCount) {
        if trim_count.download_queue > 0 {
            self.trim_download_queue
                .fetch_add(trim_count.download_queue as u64, Relaxed);
        }
        if trim_count.blocked > 0 {
            self.trim_blocked
                .fetch_add(trim_count.blocked as u64, Relaxed);
        }
    }
}

impl StatsSource for BootstrapQueueStats {
    fn collect_stats(&self, result: &mut StatsCollection) {
        result.insert(KEY, "inserted", self.inserted.load(Relaxed));
        result.insert(KEY, "upgraded", self.upgraded.load(Relaxed));
        result.insert(KEY, "removed", self.removed.load(Relaxed));
        result.insert(KEY, "remove_failed", self.remove_failed.load(Relaxed));
        result.insert(KEY, "deprioritized", self.deprioritized.load(Relaxed));
        result.insert(KEY, "not_found", self.not_found.load(Relaxed));
        result.insert(KEY, "blocked", self.blocked.load(Relaxed));
        result.insert(KEY, "block_failed", self.block_failed.load(Relaxed));
        result.insert(KEY, "unblocked", self.unblocked.load(Relaxed));
        result.insert(KEY, "decayed_blocked", self.decayed_blocked.load(Relaxed));
        result.insert(
            KEY,
            "dependency_requested",
            self.dependency_requested.load(Relaxed),
        );
        result.insert(
            KEY,
            "dependency_update",
            self.dependency_update.load(Relaxed),
        );
        result.insert(KEY, "download_started", self.download_started.load(Relaxed));
        result.insert(
            KEY,
            "download_start_failed",
            self.download_start_failed.load(Relaxed),
        );
        result.insert(
            KEY,
            "download_finished",
            self.download_finished.load(Relaxed),
        );
        result.insert(
            KEY,
            "download_finished_failed",
            self.download_finished_failed.load(Relaxed),
        );
        result.insert(
            KEY,
            "processing_started",
            self.processing_started.load(Relaxed),
        );
        result.insert(
            KEY,
            "processing_started_reverted",
            self.processing_started_reverted.load(Relaxed),
        );
        result.insert(KEY, "reprocess", self.reprocess.load(Relaxed));
        result.insert(KEY, "reprocess_failed", self.reprocess_failed.load(Relaxed));
        result.insert(
            KEY,
            "processing_finished",
            self.processing_finished.load(Relaxed),
        );
        result.insert(
            KEY,
            "processing_finished_failed",
            self.processing_finished_failed.load(Relaxed),
        );
        result.insert(
            KEY,
            "trim_download_queue",
            self.trim_download_queue.load(Relaxed),
        );
        result.insert(KEY, "trim_blocked", self.trim_blocked.load(Relaxed));
    }
}

static KEY: &str = "bootstrap_queue";
