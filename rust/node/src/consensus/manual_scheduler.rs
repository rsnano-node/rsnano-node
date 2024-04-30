use super::{ActiveTransactions, ActiveTransactionsExt, ElectionBehavior};
use crate::stats::{DetailType, Direction, StatType, Stats};
use rsnano_core::{
    utils::{ContainerInfo, ContainerInfoComponent},
    Amount, BlockEnum,
};
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub struct ManualScheduler {
    thread: Mutex<Option<JoinHandle<()>>>,
    condition: Condvar,
    mutex: Mutex<ManualSchedulerImpl>,
    stats: Arc<Stats>,
    active: Arc<ActiveTransactions>,
}

impl ManualScheduler {
    pub fn new(stats: Arc<Stats>, active: Arc<ActiveTransactions>) -> Self {
        Self {
            thread: Mutex::new(None),
            condition: Condvar::new(),
            stats,
            active,
            mutex: Mutex::new(ManualSchedulerImpl {
                queue: Default::default(),
                stopped: false,
            }),
        }
    }

    pub fn stop(&self) {
        self.mutex.lock().unwrap().stopped = true;
        self.notify();
        if let Some(handle) = self.thread.lock().unwrap().take() {
            handle.join().unwrap();
        }
    }

    pub fn notify(&self) {
        self.condition.notify_all();
    }

    pub fn push(
        &self,
        block: Arc<BlockEnum>,
        previous_balance: Option<Amount>,
        election_behavior: ElectionBehavior,
    ) {
        let mut guard = self.mutex.lock().unwrap();
        guard
            .queue
            .push_back((block, previous_balance, election_behavior));
        self.notify();
    }

    fn run(&self) {
        let mut guard = self.mutex.lock().unwrap();
        while !guard.stopped {
            guard = self
                .condition
                .wait_while(guard, |g| !g.stopped && !g.predicate())
                .unwrap();

            if !guard.stopped {
                self.stats
                    .inc(StatType::ElectionScheduler, DetailType::Loop);

                if guard.predicate() {
                    let (block, _previous_balance, election_behavior) =
                        guard.queue.pop_front().unwrap();

                    drop(guard);

                    self.stats
                        .inc(StatType::ElectionScheduler, DetailType::InsertManual);

                    let (_inserted, election) = self.active.insert(&block, election_behavior);
                    if let Some(election) = election {
                        election.transition_active();
                    }
                } else {
                    drop(guard);
                }
                self.notify();
                guard = self.mutex.lock().unwrap();
            }
        }
    }

    pub fn collect_container_info(&self, name: impl Into<String>) -> ContainerInfoComponent {
        let guard = self.mutex.lock().unwrap();
        ContainerInfoComponent::Composite(
            name.into(),
            vec![ContainerInfoComponent::Leaf(ContainerInfo {
                name: "queue".to_string(),
                count: guard.queue.len(),
                sizeof_element: size_of::<Arc<BlockEnum>>()
                    + size_of::<Option<Amount>>()
                    + size_of::<ElectionBehavior>(),
            })],
        )
    }
}

impl Drop for ManualScheduler {
    fn drop(&mut self) {
        // Thread must be stopped before destruction
        debug_assert!(self.thread.lock().unwrap().is_none());
    }
}

pub trait ManualSchedulerExt {
    fn start(&self);
}

impl ManualSchedulerExt for Arc<ManualScheduler> {
    fn start(&self) {
        debug_assert!(self.thread.lock().unwrap().is_none());
        let self_l = Arc::clone(self);
        *self.thread.lock().unwrap() = Some(
            std::thread::Builder::new()
                .name("Sched Manual".to_string())
                .spawn(Box::new(move || {
                    self_l.run();
                }))
                .unwrap(),
        )
    }
}

struct ManualSchedulerImpl {
    queue: VecDeque<(Arc<BlockEnum>, Option<Amount>, ElectionBehavior)>,
    stopped: bool,
}

impl ManualSchedulerImpl {
    fn predicate(&self) -> bool {
        !self.queue.is_empty()
    }
}