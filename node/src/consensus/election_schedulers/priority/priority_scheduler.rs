use std::{
    sync::{
        Arc, Condvar, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
};

use rsnano_ledger::{AnySet, ConfirmedSet};
use rsnano_nullable_clock::SteadyClock;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Account, AccountInfo, Amount, BlockHash, ConfirmationHeightInfo, SavedBlock};
use rsnano_utils::{
    container_info::ContainerInfo,
    stats::{DetailType, StatType, Stats, StatsCollection, StatsSource},
};

use super::{
    Bucket, Bucketing, PriorityBucketConfig, bucket_stats::BucketStats, prio_bucket_count,
    prio_bucket_index,
};
use crate::consensus::{
    AecService,
    election_schedulers::priority::{BucketInsertError, Eviction},
};

pub struct PriorityScheduler {
    stopped: Mutex<bool>,
    condition: Condvar,
    stats: Arc<Stats>,
    buckets: Mutex<Vec<Bucket>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    bucket_stats: BucketStats,
    aec_service: Arc<AecService>,
    clock: Arc<SteadyClock>,
    activate_successors_listener: OutputListenerMt<SavedBlock>,
    activations_per_bucket: Vec<AtomicU64>,
}

impl PriorityScheduler {
    pub(crate) fn new(
        config: PriorityBucketConfig,
        stats: Arc<Stats>,
        aec_service: Arc<AecService>,
        clock: Arc<SteadyClock>,
    ) -> Self {
        let mut buckets = Vec::with_capacity(prio_bucket_count());
        let mut activations_per_bucket = Vec::with_capacity(prio_bucket_count());
        for bucket_id in 0..prio_bucket_count() {
            buckets.push(Bucket::new(config.clone(), bucket_id));
            activations_per_bucket.push(AtomicU64::new(0));
        }

        Self {
            thread: Mutex::new(None),
            stopped: Mutex::new(false),
            condition: Condvar::new(),
            buckets: Mutex::new(buckets),
            stats,
            bucket_stats: BucketStats::default(),
            aec_service,
            clock,
            activate_successors_listener: Default::default(),
            activations_per_bucket,
        }
    }

    pub fn track_activate_successors(&self) -> Arc<OutputTrackerMt<SavedBlock>> {
        self.activate_successors_listener.track()
    }

    pub fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        self.condition.notify_all();
        let handle = self.thread.lock().unwrap().take();
        if let Some(handle) = handle {
            handle.join().unwrap();
        }
    }

    pub fn notify(&self) {
        self.condition.notify_all();
    }

    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.buckets
            .lock()
            .unwrap()
            .iter()
            .any(|b| b.contains(hash))
    }

    pub fn activate(&self, any: &impl AnySet, account: &Account) {
        debug_assert!(!account.is_zero());
        if let Some(account_info) = any.get_account(account) {
            let conf_info = any.confirmed().get_conf_info(account).unwrap_or_default();

            if conf_info.height < account_info.block_count {
                self.activate_with_info(any, &account_info, &conf_info);
                return;
            }
        };

        self.stats
            .inc(StatType::ElectionScheduler, DetailType::ActivateSkip);
    }

    pub fn activate_with_info(
        &self,
        any: &impl AnySet,
        account_info: &AccountInfo,
        conf_info: &ConfirmationHeightInfo,
    ) {
        debug_assert!(conf_info.frontier != account_info.head);

        let next_unconfirmed_hash = match conf_info.height {
            0 => account_info.open_block,
            _ => {
                match any.block_successor(&conf_info.frontier) {
                    Some(h) => h,
                    None => {
                        // This can happen if the bounded backlog did a rollback
                        return;
                    }
                }
            }
        };

        let Some(block) = any.get_block(&next_unconfirmed_hash) else {
            return;
        };

        if !any.dependencies_confirmed(&block) {
            self.stats
                .inc(StatType::ElectionScheduler, DetailType::ActivateFailed);
            return;
        }

        #[cfg(feature = "ledger_snapshots")]
        if any.is_forked(&block.qualified_root()) {
            self.stats
                .inc(StatType::ElectionScheduler, DetailType::ActivateFailed);
            return;
        }

        let priority = any.block_priority(&block);

        let insert_result = {
            let mut buckets = self.buckets.lock().unwrap();
            let (bucket, bucket_index) = self.find_bucket(&mut buckets, priority.balance);
            self.activations_per_bucket[bucket_index].fetch_add(1, Ordering::Relaxed);
            bucket.insert(priority, block)
        };

        match insert_result {
            Ok(Eviction::None) => {}
            Ok(Eviction::Evicted) => {
                self.stats
                    .inc(StatType::ElectionScheduler, DetailType::Evicted);
            }
            Err(BucketInsertError::Duplicate) => {
                self.stats
                    .inc(StatType::ElectionScheduler, DetailType::Duplicate);
            }
            Err(BucketInsertError::PriorityTooLow) => {
                self.stats
                    .inc(StatType::ElectionScheduler, DetailType::ActivateFull);
            }
        }

        if insert_result.is_ok() {
            self.stats
                .inc(StatType::ElectionScheduler, DetailType::Activated);
            self.condition.notify_all();
        }
    }

    fn find_bucket<'a>(
        &self,
        buckets: &'a mut [Bucket],
        priority: Amount,
    ) -> (&'a mut Bucket, usize) {
        let index = prio_bucket_index(priority);
        (&mut buckets[index], index)
    }

    pub fn len(&self) -> usize {
        self.buckets.lock().unwrap().iter().map(|b| b.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn run(&self) {
        let mut stopped = self.stopped.lock().unwrap();
        while !*stopped {
            stopped = self
                .condition
                .wait_while(stopped, |s| !*s && !self.predicate())
                .unwrap();

            if !*stopped {
                drop(stopped);
                self.run_one();
                stopped = self.stopped.lock().unwrap();
            }
        }
    }

    fn predicate(&self) -> bool {
        let buckets = self.buckets.lock().unwrap();
        buckets.iter().any(|b| b.available(&*self.aec_service))
    }

    fn run_one(&self) {
        self.stats
            .inc(StatType::ElectionScheduler, DetailType::Loop);

        let now = self.clock.now();
        let mut buckets = self.buckets.lock().unwrap();
        let mut inserted = true;

        while inserted {
            inserted = false;
            for bucket in buckets.iter_mut().rev() {
                bucket.activate(&*self.aec_service, now, &self.bucket_stats);
            }
        }
    }

    pub fn activate_successors(&self, any: &impl AnySet, block: &SavedBlock) {
        if self.activate_successors_listener.is_tracked() {
            self.activate_successors_listener.emit(block.clone());
        }
        self.activate(any, &block.account());
        self.activate_destination_account(any, block);
    }

    fn activate_destination_account(&self, any: &impl AnySet, block: &SavedBlock) {
        if let Some(destination) = block.destination()
            && block.is_send()
            && !destination.is_zero()
            && destination != block.account()
        {
            self.activate(any, &destination);
        }
    }

    pub fn container_info(&self) -> ContainerInfo {
        let mut bucket_infos = ContainerInfo::builder();

        for (id, bucket) in self.buckets.lock().unwrap().iter().enumerate() {
            bucket_infos = bucket_infos.leaf(id.to_string(), bucket.len(), 0);
        }

        ContainerInfo::builder()
            .node("blocks", bucket_infos.finish())
            .finish()
    }
}

impl Drop for PriorityScheduler {
    fn drop(&mut self) {
        // Thread must be stopped before destruction
        debug_assert!(self.thread.lock().unwrap().is_none());
    }
}

pub trait PrioritySchedulerExt {
    fn start(&self);
}

impl PrioritySchedulerExt for Arc<PriorityScheduler> {
    fn start(&self) {
        debug_assert!(self.thread.lock().unwrap().is_none());

        let self_l = Arc::clone(self);
        *self.thread.lock().unwrap() = Some(
            std::thread::Builder::new()
                .name("Sched Priority".to_string())
                .spawn(Box::new(move || {
                    self_l.run();
                }))
                .unwrap(),
        );
    }
}

impl StatsSource for PriorityScheduler {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.bucket_stats.collect_stats(result);
        for (i, activations) in self.activations_per_bucket.iter().enumerate() {
            result.insert(
                "election_bucket_activation",
                &BUCKET_NAMES[i],
                activations.load(Ordering::Relaxed),
            );
        }
    }
}

static BUCKET_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let bucket_count = Bucketing::new().bucket_count();
    let mut names = Vec::with_capacity(bucket_count);
    for i in 0..bucket_count {
        names.push(i.to_string())
    }
    names
});

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_ledger::{Ledger, LedgerInserter};
    use rsnano_types::PrivateKey;

    #[test]
    fn can_track_successor_activation() {
        let scheduler = create_test_scheduler();
        let block = SavedBlock::new_test_instance();
        let ledger = Ledger::new_null();
        let tracker = scheduler.track_activate_successors();

        scheduler.activate_successors(&ledger.any(), &block);

        let output = tracker.output();
        assert_eq!(output, [block]);
    }

    #[test]
    fn activate_successors() {
        let scheduler = create_test_scheduler();

        let ledger = Ledger::new_null();
        let inserter = LedgerInserter::new(&ledger);
        let destination = PrivateKey::from(1);
        let send1 = inserter.genesis().send(&destination, 100);
        let send2 = inserter.genesis().send(Account::from(2), 100);
        let open = inserter.account(&destination).receive(send1.hash());

        ledger.confirm(send1.hash());
        scheduler.activate_successors(&ledger.any(), &send1);
        scheduler.run_one();

        assert!(scheduler.aec_service.is_active_hash(&send2.hash()));
        assert!(scheduler.aec_service.is_active_hash(&open.hash()));
    }

    fn create_test_scheduler() -> PriorityScheduler {
        let config = PriorityBucketConfig::default();
        let stats = Arc::new(Stats::default());
        let aec_service = Arc::new(AecService::new_null());
        let clock = Arc::new(SteadyClock::new_null());
        PriorityScheduler::new(config, stats, aec_service, clock)
    }
}
