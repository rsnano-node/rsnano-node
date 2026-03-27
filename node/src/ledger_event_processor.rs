use std::sync::{Arc, RwLock, mpsc::SyncSender};

use rsnano_types::NetworkType;
use rsnano_utils::{
    BackpressureHandlerRegistry, EventHandlerRegistry,
    stats::{DetailType, StatType, Stats},
};

use crate::{
    NodeEvent,
    block_processing::{BlockProcessorQueue, LedgerPipelineEvent},
    bootstrap::Bootstrapper,
    cementation::{ConfirmingSet, ConfirmingSetEvent},
    consensus::{
        AecCooldownReason, AecService, DependentElectionsConfirmer, ForkCache, ForkCacheUpdater,
        LocalVoteHistory, election_schedulers::ElectionSchedulers,
    },
    utils::BackpressureEventProcessor,
};
use rsnano_ledger::{Ledger, LedgerEvent};

pub(crate) struct LedgerEventProcessor {
    pub(crate) node_event_sender: Option<SyncSender<NodeEvent>>,
    pub confirming_set: Arc<ConfirmingSet>,
    pub stats: Arc<Stats>,
    pub(crate) dependent_elections_confirmer: DependentElectionsConfirmer,
    pub(crate) bootstrapper: Arc<Bootstrapper>,
    pub(crate) vote_history: Arc<LocalVoteHistory>,
    pub(crate) aec_service: Arc<AecService>,
    pub(crate) block_processor_queue: Arc<BlockProcessorQueue>,
    pub(crate) fork_cache_updater: ForkCacheUpdater,
    pub(crate) election_schedulers: Arc<ElectionSchedulers>,
    pub(crate) ledger: Arc<Ledger>,
    pub(crate) plugins: EventHandlerRegistry<LedgerPipelineEvent>,
    pub(crate) backpressure_plugins: BackpressureHandlerRegistry,
}

impl LedgerEventProcessor {
    #[allow(dead_code)]
    pub fn new_null() -> Self {
        Self {
            node_event_sender: None,
            confirming_set: Arc::new(ConfirmingSet::new_null()),
            stats: Arc::new(Stats::default()),
            dependent_elections_confirmer: DependentElectionsConfirmer::new_null(),
            bootstrapper: Arc::new(Bootstrapper::new_null()),
            vote_history: Arc::new(LocalVoteHistory::new(NetworkType::NanoLiveNetwork)),
            aec_service: Arc::new(AecService::new_null()),
            block_processor_queue: Arc::new(BlockProcessorQueue::default()),
            fork_cache_updater: ForkCacheUpdater::new(Arc::new(RwLock::new(ForkCache::default()))),
            election_schedulers: ElectionSchedulers::new_null().into(),
            ledger: Ledger::new_null().into(),
            plugins: EventHandlerRegistry::default(),
            backpressure_plugins: BackpressureHandlerRegistry::default(),
        }
    }
}

impl BackpressureEventProcessor<LedgerPipelineEvent> for LedgerEventProcessor {
    fn cool_down(&mut self) {
        self.backpressure_plugins.cool_down();
        self.confirming_set.set_cooldown(true);
        self.block_processor_queue.set_cooldown(true);
        self.stats
            .inc(StatType::ConfirmingSet, DetailType::Cooldown);
    }

    fn recovered(&mut self) {
        self.backpressure_plugins.recovered();
        self.confirming_set.set_cooldown(false);
        self.block_processor_queue.set_cooldown(false);
        self.stats
            .inc(StatType::ConfirmingSet, DetailType::Recovered);
    }

    fn process(&mut self, event: LedgerPipelineEvent) {
        self.plugins.raise(&event);

        match event {
            LedgerPipelineEvent::Ledger(event) => match event {
                LedgerEvent::BlocksProcessed(results) => {
                    self.confirming_set.requeue_blocks(&results);
                    self.bootstrapper.inspect_blocks(&results);
                    self.fork_cache_updater.update(&results);
                    if let Some(sender) = &self.node_event_sender {
                        sender.send(NodeEvent::BlocksProcessed(results)).unwrap();
                    }
                }
                LedgerEvent::BlocksConfirmed(confirmed) => {
                    self.dependent_elections_confirmer
                        .confirm_dependent_elections(&confirmed);
                }
                LedgerEvent::BlocksRolledBack(rolled_back) => {
                    for result in rolled_back.iter() {
                        for block in &result.rolled_back {
                            // Stop all rolled back elections except initial
                            if block.qualified_root() != result.target_root {
                                self.aec_service.erase(&block.qualified_root());
                            }
                        }
                    }

                    self.vote_history.erase_batch(rolled_back.roots());

                    self.bootstrapper
                        .unblock_batch(rolled_back.affected_accounts());
                }
            },
            LedgerPipelineEvent::ConfirmingSet(event) => match event {
                ConfirmingSetEvent::ConfirmationFailed(hash) => {
                    // The block never got confirmed! Clean up the election, so
                    // that a new election for this block can be started
                    self.aec_service.remove_recently_confirmed(&hash);
                }
                ConfirmingSetEvent::NearFull => {
                    self.aec_service
                        .set_cooldown(true, AecCooldownReason::ConfirmingSetFull);
                }
                ConfirmingSetEvent::Recovered => {
                    self.aec_service
                        .set_cooldown(false, AecCooldownReason::ConfirmingSetFull);
                }
            },
            LedgerPipelineEvent::UnconfirmedFound(unconfirmed) => {
                let any = self.ledger.any();
                for info in unconfirmed {
                    self.election_schedulers.activate_backlog(
                        &any,
                        &info.account,
                        &info.account_info,
                        &info.conf_info,
                    );
                }
            }
        }
    }
}
