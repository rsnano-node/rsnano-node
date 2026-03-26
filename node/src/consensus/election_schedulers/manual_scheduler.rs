use std::{
    collections::VecDeque,
    mem::size_of,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

use rsnano_ledger::{AnySet, Ledger};
use rsnano_types::{Amount, Block, BlockHash, SavedBlock};
use rsnano_utils::{
    container_info::ContainerInfo,
    stats::{DetailType, StatType, Stats},
};

use crate::consensus::{AecService, election::ElectionBehavior};

pub struct ManualScheduler {
    thread: Mutex<Option<JoinHandle<()>>>,
    condition: Condvar,
    mutex: Mutex<ManualSchedulerImpl>,
    stats: Arc<Stats>,
    aec_service: Arc<AecService>,
    ledger: Arc<Ledger>,
}

impl ManualScheduler {
    pub(crate) fn new(
        stats: Arc<Stats>,
        aec_service: Arc<AecService>,
        ledger: Arc<Ledger>,
    ) -> Self {
        Self {
            thread: Mutex::new(None),
            condition: Condvar::new(),
            stats,
            aec_service,
            ledger,
            mutex: Mutex::new(ManualSchedulerImpl {
                queue: Default::default(),
                stopped: false,
            }),
        }
    }

    pub fn stop(&self) {
        self.mutex.lock().unwrap().stopped = true;
        self.notify();
        let handle = self.thread.lock().unwrap().take();
        if let Some(handle) = handle {
            handle.join().unwrap();
        }
    }

    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.mutex
            .lock()
            .unwrap()
            .queue
            .iter()
            .any(|block| block.hash() == *hash)
    }

    pub fn notify(&self) {
        self.condition.notify_all();
    }

    pub fn push(&self, block: SavedBlock) {
        {
            let mut guard = self.mutex.lock().unwrap();
            guard.queue.push_back(block);
        }
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
                    let block = guard.queue.pop_front().unwrap();
                    drop(guard);

                    let priority = self.ledger.any().block_priority(&block);
                    self.stats
                        .inc(StatType::ElectionScheduler, DetailType::InsertManual);

                    self.aec_service.activate_manual(block, priority);
                } else {
                    drop(guard);
                }
                self.notify();
                guard = self.mutex.lock().unwrap();
            }
        }
    }

    pub fn container_info(&self) -> ContainerInfo {
        let guard = self.mutex.lock().unwrap();
        [(
            "queue",
            guard.queue.len(),
            size_of::<Arc<Block>>() + size_of::<Option<Amount>>() + size_of::<ElectionBehavior>(),
        )]
        .into()
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
    queue: VecDeque<SavedBlock>,
    stopped: bool,
}

impl ManualSchedulerImpl {
    fn predicate(&self) -> bool {
        !self.queue.is_empty()
    }
}
