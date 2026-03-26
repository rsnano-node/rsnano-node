use std::{sync::Arc, time::Duration};

use rsnano_types::{BlockHash, NetworkType, Root};
use rsnano_utils::{CancellationToken, ticker::Tickable};

use super::{CpsLimiter, VoteGenerators};
use crate::consensus::{
    AecService, election::VoteType, election_schedulers::priority::bucket_count,
};

/// Creates votes for blocks within the AEC
pub(crate) struct AecVoter {
    aec_service: Arc<AecService>,
    vote_generators: Arc<VoteGenerators>,
    cps_limiter: CpsLimiter,
    current_bucket: usize,
    vote_broadcast_interval: Duration,
}

impl AecVoter {
    pub(crate) fn new(
        aec_service: Arc<AecService>,
        vote_generators: Arc<VoteGenerators>,
        network: NetworkType,
        cps_limiter: CpsLimiter,
    ) -> Self {
        Self {
            aec_service,
            vote_generators,
            cps_limiter,
            current_bucket: bucket_count() - 1,
            vote_broadcast_interval: match network {
                NetworkType::NanoDevNetwork => Duration::from_millis(500),
                _ => Duration::from_secs(15),
            },
        }
    }

    fn flush(&self, queue: &mut Vec<(Root, BlockHash, VoteType)>) {
        // TODO: enqueue with one call
        for (root, hash, vote_type) in queue.drain(..) {
            self.vote_generators.generate_vote(&root, &hash, vote_type);
        }
    }
}

impl Tickable for AecVoter {
    fn tick(&mut self, cancel_token: &CancellationToken) {
        let now = self.aec_service.now();
        let mut voted = true;
        let mut vote_queue = Vec::new();
        while voted {
            voted = false;
            loop {
                if let Some((root, winner_hash, vote_type)) = self
                    .aec_service
                    .next_vote_to_broadcast(self.current_bucket, self.vote_broadcast_interval, now)
                {
                    if vote_type == VoteType::NonFinal && !self.cps_limiter.try_vote(now) {
                        self.flush(&mut vote_queue);
                        return;
                    }

                    vote_queue.push((root, winner_hash, vote_type));
                    voted = true;
                }

                if cancel_token.is_cancelled() {
                    return;
                }

                if self.current_bucket == 0 {
                    self.current_bucket = bucket_count() - 1;
                    break;
                } else {
                    self.current_bucket -= 1;
                }
            }
        }
        self.flush(&mut vote_queue);
    }
}
