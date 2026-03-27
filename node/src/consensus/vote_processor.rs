use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use tracing::debug;

use rsnano_network::Channel;
use rsnano_types::{BlockHash, Vote, VoteError, VoteSource};
use rsnano_utils::stats::{DetailType, StatType, Stats};

use super::{AecService, FilteredVote, ReceivedVote, VoteProcessorQueue};

#[derive(Clone, Debug, PartialEq)]
pub struct VoteProcessorConfig {
    pub max_pr_queue: usize,
    pub max_non_pr_queue: usize,
    pub pr_priority: usize,
    pub threads: usize,
    pub batch_size: usize,
    pub max_triggered: usize,
}

impl VoteProcessorConfig {
    pub fn new(parallelism: usize) -> Self {
        Self {
            max_pr_queue: 256,
            max_non_pr_queue: 32,
            pr_priority: 3,
            threads: (parallelism / 2).clamp(1, 4),
            batch_size: 1024,
            max_triggered: 16384,
        }
    }
}

pub type VoteProcessedCallback2 =
    Box<dyn Fn(&Arc<Vote>, Option<&Arc<Channel>>, VoteSource, VoteError) + Send + Sync>;

pub struct VoteProcessor {
    threads: Mutex<Vec<JoinHandle<()>>>,
    queue: Arc<VoteProcessorQueue>,
    aec_service: Arc<AecService>,
    stats: Arc<Stats>,
    pub total_processed: AtomicU64,
    cool_down: AtomicBool,
}

impl VoteProcessor {
    pub(crate) fn new(
        queue: Arc<VoteProcessorQueue>,
        aec_service: Arc<AecService>,
        stats: Arc<Stats>,
    ) -> Self {
        Self {
            queue,
            aec_service,
            stats,
            threads: Mutex::new(Vec::new()),
            total_processed: AtomicU64::new(0),
            cool_down: AtomicBool::new(false),
        }
    }

    pub fn cool_down(&self) {
        self.cool_down.store(true, Ordering::Relaxed);
    }

    pub fn recovered(&self) {
        self.cool_down.store(false, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.queue.stop();

        let mut handles = Vec::new();
        {
            let mut guard = self.threads.lock().unwrap();
            std::mem::swap(&mut handles, &mut guard);
        }
        for handle in handles {
            handle.join().unwrap()
        }
    }

    pub fn run(&self) {
        loop {
            if self.cool_down.load(Ordering::Relaxed) {
                if self.queue.stopped() {
                    return;
                }

                std::thread::sleep(Duration::from_millis(25));
                continue;
            }

            self.stats.inc(StatType::VoteProcessor, DetailType::Loop);

            let batch = self.queue.wait_for_votes(self.queue.config.batch_size);
            if batch.is_empty() {
                break; //stopped
            }

            let start = Instant::now();

            for (_, (vote, source, channel, filter)) in &batch {
                let filter = filter.unwrap_or_default();
                let received_vote = ReceivedVote::new(vote.clone(), *source, channel.clone());
                let filtered_vote = FilteredVote::new(received_vote.clone(), filter);
                let _ = self.vote_blocking(&filtered_vote);
            }

            self.total_processed
                .fetch_add(batch.len() as u64, Ordering::SeqCst);

            let elapsed_millis = start.elapsed().as_millis();
            if batch.len() == self.queue.config.batch_size && elapsed_millis > 100 {
                debug!(
                    "Processed {} votes in {} milliseconds (rate of {} votes per second)",
                    batch.len(),
                    elapsed_millis,
                    (batch.len() * 1000) / elapsed_millis as usize
                );
            }
        }
    }

    pub fn vote_blocking(&self, vote: &FilteredVote) -> Result<(), VoteError> {
        let mut result = Err(VoteError::Invalid);
        if vote.validate().is_ok() {
            let vote_results = self.aec_service.apply_vote(vote);
            result = aggregate_vote_results(&vote_results);
        }

        result
    }
}

impl Drop for VoteProcessor {
    fn drop(&mut self) {
        // Thread must be stopped before destruction
        debug_assert!(self.threads.lock().unwrap().is_empty());
    }
}

pub trait VoteProcessorExt {
    fn start(&self);
}

impl VoteProcessorExt for Arc<VoteProcessor> {
    fn start(&self) {
        let mut threads = self.threads.lock().unwrap();
        debug_assert!(threads.is_empty());
        for _ in 0..self.queue.config.threads {
            let self_l = Arc::clone(self);
            threads.push(
                std::thread::Builder::new()
                    .name("Vote processing".to_string())
                    .spawn(Box::new(move || {
                        self_l.run();
                    }))
                    .unwrap(),
            )
        }
    }
}

// Aggregate results for individual hashes
pub fn aggregate_vote_results(
    results: &HashMap<BlockHash, Result<(), VoteError>>,
) -> Result<(), VoteError> {
    let mut ignored = false;
    let mut replay = false;
    let mut processed = false;
    let mut late = false;
    for res in results.values() {
        ignored |= matches!(res, Err(VoteError::Ignored));
        replay |= matches!(res, Err(VoteError::Replay));
        processed |= res.is_ok();
        late |= matches!(res, Err(VoteError::Late));
    }
    if ignored {
        Err(VoteError::Ignored)
    } else if replay {
        Err(VoteError::Replay)
    } else if processed {
        Ok(())
    } else if late {
        Err(VoteError::Late)
    } else {
        Err(VoteError::Indeterminate)
    }
}
