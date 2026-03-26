mod election_schedulers_plugin;
mod hinted_scheduler;
mod manual_scheduler;
mod optimistic_scheduler;
pub mod priority;

pub(crate) use election_schedulers_plugin::*;
pub use hinted_scheduler::*;
pub use manual_scheduler::*;
pub use optimistic_scheduler::*;

use std::sync::{Arc, Mutex};

use rsnano_ledger::{AnySet, Ledger, ProcessResult};
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_types::{
    Account, AccountInfo, BlockHash, ConfirmationHeightInfo, NetworkType, SavedBlock,
};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{Stats, StatsCollection, StatsSource},
};

use super::{AecService, VoteCache};
use crate::{
    cementation::ConfirmingSet,
    config::{NetworkConstants, NodeConfig},
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
}

impl ElectionSchedulers {
    pub(crate) fn new(
        config: NodeConfig,
        network_constants: NetworkConstants,
        aec_service: Arc<AecService>,
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

        let optimistic = Arc::new(OptimisticScheduler::new(
            config.optimistic_scheduler.clone(),
            stats.clone(),
            aec_service.clone(),
            network_constants,
            ledger.clone(),
            confirming_set.clone(),
        ));

        let priority = Arc::new(PriorityScheduler::new(
            config.priority_bucket.clone(),
            stats.clone(),
            aec_service,
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
        }
    }

    pub fn new_null() -> Self {
        let config = NodeConfig::new_test_instance();
        let network_constants = NetworkConstants::for_network(NetworkType::NanoLiveNetwork);
        let ledger = Arc::new(Ledger::new_null());
        let stats = Arc::new(Stats::default());
        let vote_cache = Arc::new(Mutex::new(VoteCache::new(
            Default::default(),
            stats.clone(),
        )));
        let confirming_set = Arc::new(ConfirmingSet::new_null());
        let online_reps = Arc::new(Mutex::new(OnlineReps::new_test_instance()));
        let aec_service = Arc::new(AecService::new_null());

        Self::new(
            config,
            network_constants,
            aec_service,
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
        self.optimistic.activate(account, account_info, conf_info);
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
            self.optimistic.start();
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
