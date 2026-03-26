use std::sync::{Arc, Mutex, RwLock};

use tracing::debug;

use crate::block_processing::LedgerPipelineEvent;
use rsnano_ledger::{BlockError, LedgerEvent, ProcessResult, RepWeightCache};
use rsnano_types::{Amount, Block, BlockHash, QualifiedRoot};
use rsnano_utils::{EventHandlerMut, stats::Stats};

use super::{AecService, ForkCache, VoteCache};

pub(crate) struct AecForkInserter {
    pub(crate) rep_weights: Arc<RepWeightCache>,
    pub(crate) fork_cache: Arc<RwLock<ForkCache>>,
    pub(crate) aec_service: Arc<AecService>,
    pub(crate) vote_cache: Arc<Mutex<VoteCache>>,
}

impl AecForkInserter {
    #[allow(dead_code)]
    pub fn new_test_instance() -> Self {
        Self {
            rep_weights: Arc::new(RepWeightCache::default()),
            fork_cache: Arc::new(RwLock::new(ForkCache::new())),
            aec_service: Arc::new(AecService::new_null()),
            vote_cache: Arc::new(Mutex::new(VoteCache::new(
                Default::default(),
                Arc::new(Stats::default()),
            ))),
        }
    }

    pub fn handle_forks(&self, batch: &[ProcessResult]) {
        for result in batch {
            if result.status == Err(BlockError::Fork) {
                self.handle_fork(&result.block);
            }
        }
    }

    pub fn try_add_cached_forks(&self, root: &QualifiedRoot) {
        let fork_cache = self.fork_cache.read().unwrap();
        for fork in fork_cache.get_forks(root) {
            self.handle_fork(fork);
        }
    }

    fn handle_fork(&self, fork: &Block) {
        let fork_tally = self.get_cached_tally(&fork.hash());

        let added = self.aec_service.try_add_fork(fork, fork_tally);

        if added {
            debug!("Block was added to an existing election: {}", fork.hash());
        }
    }

    fn get_cached_tally(&self, hash: &BlockHash) -> Amount {
        let votes = self.vote_cache.lock().unwrap().find(hash);
        let mut tally = Amount::ZERO;
        let weights = self.rep_weights.read();
        for vote in votes {
            tally += weights.weight(&vote.voter);
        }
        tally
    }
}

pub(crate) struct ForkInserterPlugin {
    fork_processor: Arc<AecForkInserter>,
}

impl ForkInserterPlugin {
    pub fn new(fork_processor: Arc<AecForkInserter>) -> Self {
        Self { fork_processor }
    }
}

impl EventHandlerMut<LedgerPipelineEvent> for ForkInserterPlugin {
    fn handle(&mut self, event: &LedgerPipelineEvent) {
        if let LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(results)) = event {
            // Notify elections about alternative (forked) blocks
            self.fork_processor.handle_forks(results);
        }
    }
}
