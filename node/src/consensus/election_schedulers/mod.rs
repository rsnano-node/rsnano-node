mod election_schedulers_plugin;
mod hinted_scheduler;
mod manual_scheduler;
mod optimistic;
pub mod priority;

pub(crate) use election_schedulers_plugin::*;
pub use hinted_scheduler::*;
pub use manual_scheduler::*;
pub use optimistic::*;

use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use rsnano_ledger::{AnySet, Ledger, ProcessResult};
use rsnano_nullable_clock::SteadyClock;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{Account, AccountInfo, BlockHash, ConfirmationHeightInfo, SavedBlock};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{Stats, StatsCollection, StatsSource},
};

use super::{AecService, VoteCache};
use crate::{
    cementation::ConfirmingSet,
    config::NodeConfig,
    representatives::OnlineReps,
};
use priority::{PriorityScheduler, PrioritySchedulerExt};

pub struct ElectionSchedulers {
    pub priority: Arc<PriorityScheduler>,
    pub optimistic: Arc<OptimisticScheduler>,
    pub hinted: Arc<HintedScheduler>,
    pub manual: Arc<ManualScheduler>,
    notify_listener: OutputListenerMt<()>,
    config: NodeConfig,
    ledger: Arc<Ledger>,
    activate_successors_listener: OutputListenerMt<SavedBlock>,
    optimistic_thread: Mutex<Option<JoinHandle<()>>>,
}

impl ElectionSchedulers {
    pub(crate) fn new(
        config: NodeConfig,
        aec_service: Arc<AecService>,
        clock: Arc<SteadyClock>,
        ledger: Arc<Ledger>,
        stats: Arc<Stats>,
        vote_cache: Arc<Mutex<VoteCache>>,
        confirming_set: Arc<ConfirmingSet>,
        online_reps: Arc<Mutex<OnlineReps>>,
    ) -> Self {
        let hinted = Arc::new(HintedScheduler::new(
            config.hinted_scheduler.clone(),
            aec_service.clone(),
            ledger.clone(),
            stats.clone(),
            vote_cache.clone(),
            confirming_set.clone(),
            online_reps.clone(),
        ));

        let manual = Arc::new(ManualScheduler::new(
            stats.clone(),
            aec_service.clone(),
            ledger.clone(),
        ));

        let optimistic_params = OptimisticSchedulerParams {
            gap_threshold: config.optimistic_scheduler.gap_threshold,
            max_candidates: config.optimistic_scheduler.max_size,
            max_elections: config.active_elections.max_elections
                * config.optimistic_scheduler.optimistic_limit_percentage
                / 100,
            activation_delay: config.optimistic_scheduler.activation_delay,
        };
        let optimistic = Arc::new(OptimisticScheduler::new(
            optimistic_params,
            aec_service.clone(),
            clock.clone(),
            ledger.clone(),
            confirming_set.clone(),
        ));

        let priority = Arc::new(PriorityScheduler::new(
            config.priority_bucket.clone(),
            stats.clone(),
            aec_service,
            clock,
        ));

        Self {
            priority,
            optimistic,
            hinted,
            manual,
            notify_listener: OutputListenerMt::new(),
            config,
            ledger,
            activate_successors_listener: Default::default(),
            optimistic_thread: Mutex::new(None),
        }
    }

    pub fn new_null() -> Self {
        let config = NodeConfig::new_test_instance();
        let ledger = Arc::new(Ledger::new_null());
        let stats = Arc::new(Stats::default());
        let vote_cache = Arc::new(Mutex::new(VoteCache::new(
            Default::default(),
            stats.clone(),
        )));
        let confirming_set = Arc::new(ConfirmingSet::new_null());
        let online_reps = Arc::new(Mutex::new(OnlineReps::new_test_instance()));
        let aec_service = Arc::new(AecService::new_null());
        let clock = Arc::new(SteadyClock::new_null());

        Self::new(
            config,
            aec_service,
            clock,
            ledger,
            stats,
            vote_cache,
            confirming_set,
            online_reps,
        )
    }

    pub fn track_activate_successors(&self) -> Arc<OutputTrackerMt<SavedBlock>> {
        self.activate_successors_listener.track()
    }

    /// Does the block exist in any of the schedulers
    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.manual.contains(hash) || self.priority.contains(hash)
    }

    pub fn activate_backlog(
        &self,
        any: &impl AnySet,
        account: &Account,
        account_info: &AccountInfo,
        conf_info: &ConfirmationHeightInfo,
    ) {
        self.optimistic
            .activate(account, account_info.block_count, conf_info.height);
        self.priority
            .activate_with_info(any, account_info, conf_info);
    }

    pub fn activate_accounts_with_fresh_blocks(&self, processed: &[ProcessResult]) {
        let any = self.ledger.any();
        for result in processed {
            if result.status.is_ok() {
                let account = result.saved_block.as_ref().unwrap().account();
                self.priority.activate(&any, &account);
            }
        }
    }

    pub fn notify(&self) {
        self.notify_listener.emit(());
        self.priority.notify();
        self.hinted.notify();
        self.optimistic.notify();
    }

    pub fn add_manual(&self, block: SavedBlock) {
        self.manual.push(block);
    }

    pub fn activate_successors<'a>(&self, confirmed: impl IntoIterator<Item = &'a SavedBlock>) {
        // Activate successors of confirmed blocks
        let any = self.ledger.any();
        for block in confirmed {
            if self.activate_successors_listener.is_tracked() {
                self.activate_successors_listener.emit(block.clone());
            }
            self.priority.activate_successors(&any, block);
        }
    }

    pub fn start(&self) {
        if self.config.enable_hinted_scheduler {
            self.hinted.start();
        }
        self.manual.start();
        if self.config.enable_optimistic_scheduler {
            let optimistic = self.optimistic.clone();
            let handle = std::thread::Builder::new()
                .name("Sched Opt".to_string())
                .spawn(move || optimistic.run_loop())
                .unwrap();
            *self.optimistic_thread.lock().unwrap() = Some(handle);
        }
        if self.config.enable_priority_scheduler {
            self.priority.start();
        }
    }

    pub fn track_notify(&self) -> Arc<OutputTrackerMt<()>> {
        self.notify_listener.track()
    }

    pub fn stop(&self) {
        self.hinted.stop();
        self.manual.stop();
        self.optimistic.stop();
        if let Some(handle) = self.optimistic_thread.lock().unwrap().take() {
            handle.join().unwrap();
        }
        self.priority.stop();
    }
}

impl ContainerInfoProvider for ElectionSchedulers {
    fn container_info(&self) -> ContainerInfo {
        ContainerInfo::builder()
            .node("hinted", self.hinted.container_info())
            .node("manual", self.manual.container_info())
            .node("optimistic", self.optimistic.container_info())
            .node("priority", self.priority.container_info())
            .finish()
    }
}

impl StatsSource for ElectionSchedulers {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.priority.collect_stats(result);
        self.optimistic.collect_stats(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_successors() {
        let schedulers = ElectionSchedulers::new_null();
        let tracker = schedulers.priority.track_activate_successors();
        let block = SavedBlock::new_test_instance();

        schedulers.activate_successors([&block]);

        let output = tracker.output();
        assert_eq!(output, [block]);
    }

    #[test]
    fn can_track_successor_activation() {
        let schedulers = ElectionSchedulers::new_null();
        let tracker = schedulers.track_activate_successors();
        let block = SavedBlock::new_test_instance();

        schedulers.activate_successors([&block]);

        let output = tracker.output();
        assert_eq!(output, [block]);
    }
}
