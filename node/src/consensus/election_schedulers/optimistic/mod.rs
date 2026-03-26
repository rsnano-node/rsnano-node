use std::{
    sync::{Arc, atomic::Ordering::Relaxed},
    time::Duration,
};

use rsnano_ledger::{AnySet, Ledger, LedgerSet, OwningAnySet};
use rsnano_nullable_condvar::NullableCondvarMutex;
use rsnano_types::Account;
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{StatsCollection, StatsSource},
};

use crate::{
    cementation::ConfirmingSet,
    consensus::{AecService, election::ElectionBehavior},
};

mod candidate_queue;
mod config;
mod logic;
mod stats;

pub use config::OptimisticSchedulerParams;
use logic::OptimisticSchedulerLogic;
use stats::OptimisticSchedulerStats;

pub struct OptimisticScheduler {
    logic: NullableCondvarMutex<OptimisticSchedulerLogic>,
    aec_service: Arc<AecService>,
    ledger: Arc<Ledger>,
    confirming_set: Arc<ConfirmingSet>,
    max_elections: usize,
    activation_delay: Duration,
    stats: OptimisticSchedulerStats,
}

impl OptimisticScheduler {
    pub(crate) fn new(
        params: OptimisticSchedulerParams,
        aec_service: Arc<AecService>,
        ledger: Arc<Ledger>,
        confirming_set: Arc<ConfirmingSet>,
    ) -> Self {
        Self {
            max_elections: params.max_elections,
            activation_delay: params.activation_delay,
            logic: NullableCondvarMutex::new(OptimisticSchedulerLogic::new(params)),
            aec_service,
            ledger,
            confirming_set,
            stats: Default::default(),
        }
    }

    pub fn max_elections(&self) -> usize {
        self.max_elections
    }

    pub fn stop(&self) {
        self.logic.lock().stop();
        self.logic.notify_all();
    }

    /// Notify about changes in AEC vacancy
    pub fn notify(&self) {
        self.logic.notify_all();
    }

    /// Called from backlog population to process accounts with unconfirmed blocks
    pub fn activate(&self, account: &Account, block_count: u64, confirmation_height: u64) -> bool {
        let now = self.aec_service.now();
        let mut logic = self.logic.lock();
        let activated = logic.try_activate(account, block_count, confirmation_height, now);
        if activated {
            self.stats.activated_count.fetch_add(1, Relaxed);
        }
        activated
    }

    pub fn run_loop(&self) {
        let mut logic = self.logic.lock();
        while !logic.stopped() {
            self.stats.loop_count.fetch_add(1, Relaxed);

            if self.can_schedule(&logic) {
                let any = self.ledger.any();

                while self.can_schedule(&logic) {
                    if let Some(account) = logic.pop_candidate() {
                        drop(logic);
                        self.run_one(&any, account);
                        logic = self.logic.lock();
                    } else {
                        break;
                    }
                }
            }

            logic = self
                .logic
                .wait_timeout_while(logic, self.activation_delay / 2, |g| {
                    !g.stopped() && !self.can_schedule(g)
                })
                .0;
        }
    }

    fn run_one(&self, any: &OwningAnySet, account: Account) {
        let Some(head) = any.account_head(&account) else {
            return;
        };
        let Some(block) = any.get_block(&head) else {
            return;
        };

        #[cfg(feature = "ledger_snapshots")]
        {
            if any.is_forked(&block.qualified_root()) {
                // Needed for new consensus algorithm in ledger snapshot.
                // We never vote for forked blocks.
                return;
            }
        }

        // Ensure block is not already confirmed
        let is_confirmed = self.confirming_set.contains(&block.hash())
            || any.confirmed().block_exists(&block.hash());

        if is_confirmed {
            // No need to schedule an election if already confirmed.
            return;
        }
        // Try to insert it into AEC
        // We check for AEC vacancy inside our predicate
        let priority = any.block_priority(&block);
        let inserted = self.aec_service.insert_optimistic(block, priority);

        if inserted {
            self.stats.insert_count.fetch_add(1, Relaxed);
        } else {
            self.stats.insert_failed_count.fetch_add(1, Relaxed);
        }
    }

    fn can_schedule(&self, logic: &OptimisticSchedulerLogic) -> bool {
        logic.can_schedule(
            self.aec_service
                .count_by_behavior(ElectionBehavior::Optimistic),
            self.aec_service.vacancy(),
            self.aec_service.now(),
        )
    }

    #[cfg(test)]
    pub fn candidate_count(&self) -> usize {
        self.logic.lock().candidate_count()
    }
}

impl StatsSource for OptimisticScheduler {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.stats.collect_stats(result);
    }
}

impl ContainerInfoProvider for OptimisticScheduler {
    fn container_info(&self) -> ContainerInfo {
        self.logic.lock().container_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_ledger::{ConfirmedSet, test_helpers::UnsavedBlockLatticeBuilder};
    use rsnano_nullable_condvar::NotifyEvent;

    #[test]
    fn stop_sets_stopped_flag_and_notifies() {
        let scheduler = make_scheduler();
        let tracker = scheduler.logic.track_notifications();

        scheduler.stop();

        assert!(scheduler.logic.lock().stopped());
        assert_eq!(tracker.output(), vec![NotifyEvent::NotifyAll]);
    }

    #[test]
    fn notify_sends_notify_all() {
        let scheduler = make_scheduler();
        let tracker = scheduler.logic.track_notifications();

        scheduler.notify();

        assert_eq!(tracker.output(), vec![NotifyEvent::NotifyAll]);
    }

    #[test]
    fn schedules_election_when_over_gap_threshold() {
        let logic =
            NullableCondvarMutex::null_builder(OptimisticSchedulerLogic::new(test_params()))
                .wait(|l| l.stop()) // stop after one wait call
                .finish();

        let aec_service = Arc::new(AecService::new_null());
        let ledger = Arc::new(Ledger::new_null());
        let scheduler = make_scheduler_with(logic, aec_service.clone(), ledger.clone());

        let mut builder = UnsavedBlockLatticeBuilder::with_stub_work();
        for _ in 0..TEST_GAP_THRESHOLD {
            let block = builder.genesis().send(1, 1);
            ledger.process_one(&block).unwrap();
        }

        let account = ledger.genesis().account();
        let block_count = ledger.any().get_account(&account).unwrap().block_count;
        let conf_height = ledger.confirmed().get_conf_info(&account).unwrap().height;
        assert!(scheduler.activate(&account, block_count, conf_height));

        scheduler.run_loop();

        let optimistic_count = aec_service.count_by_behavior(ElectionBehavior::Optimistic);

        assert_eq!(optimistic_count, 1, "should schedule the election");
        assert_eq!(
            scheduler.candidate_count(),
            0,
            "should remove the candidate"
        );
    }

    #[test]
    #[ignore = "TODO"]
    fn schedules_election_when_account_is_unconfirmed() {
        // This should replace the test activate_one_zero_conf
    }

    /* Test helpers */

    fn make_scheduler() -> OptimisticScheduler {
        OptimisticScheduler::new(
            test_params(),
            Arc::new(AecService::new_null()),
            Ledger::new_null().into(),
            ConfirmingSet::new_null().into(),
        )
    }

    fn make_scheduler_with(
        logic: NullableCondvarMutex<OptimisticSchedulerLogic>,
        aec_service: Arc<AecService>,
        ledger: Arc<Ledger>,
    ) -> OptimisticScheduler {
        OptimisticScheduler {
            logic,
            aec_service,
            ledger,
            confirming_set: ConfirmingSet::new_null().into(),
            max_elections: 10,
            stats: Default::default(),
            activation_delay: Duration::ZERO,
        }
    }

    fn test_params() -> OptimisticSchedulerParams {
        OptimisticSchedulerParams {
            gap_threshold: TEST_GAP_THRESHOLD,
            max_candidates: 1024,
            max_elections: 10,
            activation_delay: Duration::ZERO,
        }
    }

    const TEST_GAP_THRESHOLD: u64 = 32;
}
