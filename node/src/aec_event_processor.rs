use std::sync::{Arc, Mutex, mpsc::SyncSender};

use tracing::debug;

use rsnano_ledger::BlockSource;
use rsnano_messages::NetworkFilter;
use rsnano_network::ChannelId;
use rsnano_nullable_clock::SteadyClock;
use rsnano_types::{Block, VoteError, VoteSource};
use rsnano_utils::stats::{Sample, Stats};

use crate::{
    NodeEvent,
    block_processing::{BlockContext, BlockProcessorQueue},
    cementation::ConfirmingSet,
    consensus::{
        AecCooldownReason, AecEvent, AecForkInserter, AecService, BootstrapElectionActivator,
        LocalVotesRemover, ReceivedVote, VoteCache, VoteCacheProcessor, VoteProcessor,
        VoteRebroadcastQueue, WinnerBlockBroadcaster, aggregate_vote_results,
        election_schedulers::ElectionSchedulers,
    },
    recently_cemented_inserter::RecentlyCementedInserter,
    representatives::{OnlineReps, RepCrawler},
    utils::BackpressureEventProcessor,
};

/// Processes events from the active election container (AEC)
pub(crate) struct AecEventProcessor {
    pub(crate) vote_cache_processor: Arc<VoteCacheProcessor>,
    pub(crate) vote_processor: Arc<VoteProcessor>,
    pub(crate) vote_cache: Arc<Mutex<VoteCache>>,
    pub(crate) node_observer: Option<SyncSender<NodeEvent>>,
    pub(crate) election_schedulers: Arc<ElectionSchedulers>,
    pub(crate) network_filter: Arc<NetworkFilter>,
    pub(crate) bootstrap_election_activator: BootstrapElectionActivator,
    pub(crate) recently_cemented_inserter: RecentlyCementedInserter,
    pub(crate) vote_rebroadcast_queue: Arc<VoteRebroadcastQueue>,
    pub(crate) block_processor_queue: Arc<BlockProcessorQueue>,
    pub(crate) confirming_set: Arc<ConfirmingSet>,
    pub(crate) online_reps: Arc<Mutex<OnlineReps>>,
    pub(crate) aec_service: Arc<AecService>,
    pub(crate) rep_crawler: Arc<RepCrawler>,
    pub(crate) clock: Arc<SteadyClock>,
    pub(crate) local_votes_remover: LocalVotesRemover,
    pub(crate) stats: Arc<Stats>,
    pub(crate) aec_fork_inserter: Arc<AecForkInserter>,
    pub(crate) winner_block_broadcaster: Arc<Mutex<WinnerBlockBroadcaster>>,
}

impl BackpressureEventProcessor<AecEvent> for AecEventProcessor {
    fn cool_down(&mut self) {
        self.aec_service
            .set_cooldown(true, AecCooldownReason::AecEventQueueFull);
        self.vote_processor.cool_down();
    }

    fn recovered(&mut self) {
        self.aec_service
            .set_cooldown(false, AecCooldownReason::AecEventQueueFull);
        self.vote_processor.recovered();
    }

    fn process(&mut self, event: AecEvent) {
        match event {
            AecEvent::ElectionStarted(hash, root) => {
                self.aec_fork_inserter.try_add_cached_forks(&root);
                self.bootstrap_election_activator.election_started(hash);
                self.vote_cache_processor.trigger(hash);
                if let Some(tx) = &self.node_observer {
                    tx.send(NodeEvent::ElectionStarted(hash)).unwrap();
                }
            }
            AecEvent::ElectionConfirmed(election) => {
                self.confirming_set.add(election.clone());
                // Ensure election winner is broadcasted
                self.winner_block_broadcaster
                    .lock()
                    .unwrap()
                    .try_broadcast_winner(&election.winner, &election.votes);
            }
            AecEvent::ElectionEnded(election) => {
                self.election_schedulers.notify();

                let now = self.clock.now();
                let elapsed = election.start().elapsed(now);
                // Track election duration
                self.stats.sample(
                    Sample::ActiveElectionDuration,
                    elapsed.as_millis() as i64,
                    (0, 1000 * 60 * 10),
                ); // 0-10 minutes range

                for (hash, block) in election.candidate_blocks() {
                    // Notify observers about dropped elections & blocks lost confirmed elections
                    if (!election.is_confirmed() || *hash != election.winner().hash())
                        && let Some(tx) = &self.node_observer
                    {
                        tx.send(NodeEvent::ElectionStopped(*hash)).unwrap();
                    }

                    if !election.is_confirmed() {
                        self.clear_network_filter(block);
                    }
                }
            }
            AecEvent::BlockAddedToElection(hash) => self.vote_cache_processor.trigger(hash),
            AecEvent::BlockDiscarded(block) => {
                self.clear_network_filter(&block);
            }
            AecEvent::WinnerChanged(previous_winner, new_winner) => {
                debug!(from = ?previous_winner, to = ?new_winner.hash(), "Winning fork changed");
                self.local_votes_remover
                    .remove_local_votes(&previous_winner, &new_winner.qualified_root());

                // Roll back the previous winner and add the new winner to the ledger
                self.block_processor_queue.push(BlockContext::new(
                    new_winner.clone(),
                    BlockSource::Forced,
                    ChannelId::LOOPBACK,
                ));
            }
            AecEvent::VoteProcessed(vote, voter_weight, results) => {
                // Cache the votes that didn't match any election
                if vote.source != VoteSource::Cache {
                    self.vote_cache
                        .lock()
                        .unwrap()
                        .insert(&vote.vote, voter_weight, &results);
                }

                self.vote_rebroadcast_queue
                    .try_enqueue(&vote.vote, &results);

                let result = aggregate_vote_results(&results);
                self.try_update_online_reps(&vote, result);

                if let Some(tx) = &self.node_observer {
                    tx.send(NodeEvent::VoteProcessed(vote.vote, result))
                        .unwrap();
                }
            }
            AecEvent::BlockConfirmed(block, election) => {
                if let Some(tx) = &self.node_observer {
                    tx.send(NodeEvent::BlockConfirmed(block, election.clone()))
                        .unwrap();
                }
                self.recently_cemented_inserter.insert(election);
            }
            AecEvent::Recovered => self.election_schedulers.notify(),
        }
    }
}

impl AecEventProcessor {
    fn clear_network_filter(&mut self, block: &Block) {
        let mut buffer = Vec::new();
        block
            .serialize_without_block_type(&mut buffer)
            .expect("Should serialize block successfully");
        self.network_filter.clear_bytes(&buffer);
    }

    fn try_update_online_reps(&mut self, vote: &ReceivedVote, result: Result<(), VoteError>) {
        // Track rep weight voting on live elections
        let mut should_observe = matches!(
            result,
            Ok(()) | Err(VoteError::Replay) | Err(VoteError::Ignored)
        );

        // Ignore republished votes when rep crawling
        if vote.source == VoteSource::Live {
            should_observe |= self.rep_crawler.process(vote);
        }

        if should_observe {
            // Representative is defined as online if replying to live votes or rep_crawler queries
            self.online_reps
                .lock()
                .unwrap()
                .vote_observed(vote.voter, self.clock.now());
        }
    }
}
