pub mod query_tracker;

mod block_inspector;
mod bootstrap_queue;
mod frontier_scan;
mod requesters;
mod response_processor;

pub use frontier_scan::{FrontierHeadInfo, FrontierScanConfig};

pub use bootstrap_queue::{
    BootstrapQueueConfig, BootstrapQueueInfo, BootstrapQueueSnapshot, BootstrappingAccountInfo,
};

use std::{
    sync::{Arc, Mutex, RwLock},
    thread::JoinHandle,
    time::Duration,
};

use tracing::{trace, warn};

use rsnano_ledger::{Ledger, LedgerEvent, LedgerSet, ProcessResult};
use rsnano_messages::{AscPullAck, BlocksAckPayload};
use rsnano_messages::{AscPullReqType, FrontiersReqPayload, HashType};
use rsnano_network::{Channel, ChannelId, DeadChannelCleanupStep, Network};
use rsnano_nullable_clock::SteadyClock;
use rsnano_nullable_condvar::NullableCondvarMutex;
use rsnano_types::{Account, BlockHash};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{DetailType, Sample, StatType, Stats, StatsCollection, StatsSource},
    EventHandler,
};

use crate::{
    block_processing::{BlockProcessorQueue, LedgerPipelineEvent},
    transport::MessageSender,
};

use block_inspector::BlockInspector;
use bootstrap_queue::BootstrapQueue;
use bootstrap_queue::Priority;
use frontier_scan::FrontierScan;
use query_tracker::{ProcessError, QueryTracker, QueryType};
use requesters::Requesters;
use response_processor::ResponseProcessor;

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct AscPullQuerySpec {
    pub query_id: u64,
    pub channel: Arc<Channel>,
    pub req_type: AscPullReqType,
    pub account: Account,
    pub hash: BlockHash,
}

impl AscPullQuerySpec {
    #[allow(dead_code)]
    pub fn new_test_instance() -> Self {
        Self {
            query_id: 123567,
            req_type: AscPullReqType::Frontiers(FrontiersReqPayload {
                start: 100.into(),
                count: 1000,
            }),
            channel: Arc::new(Channel::new_test_instance()),
            account: Account::from(100),
            hash: BlockHash::from(200),
        }
    }

    pub fn query_type(&self) -> QueryType {
        match &self.req_type {
            AscPullReqType::Blocks(b) => match b.start_type {
                HashType::Account => QueryType::BlocksByAccount,
                HashType::Block => QueryType::BlocksByHash,
            },
            AscPullReqType::AccountInfo(_) => QueryType::AccountInfoByHash,
            AscPullReqType::Frontiers(_) => QueryType::Frontiers,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BootstrapConfig {
    pub enable: bool,
    pub enable_block_requester: bool,
    pub enable_dependency_walker: bool,
    pub enable_frontier_scan: bool,
    /// Maximum number of un-responded requests per channel, should be lower or equal to bootstrap server max queue size
    pub channel_limit: usize,
    /// Limit of outgoing messages/s over all channels
    pub rate_limit: usize,
    pub database_rate_limit: usize,
    pub frontier_rate_limit: usize,
    pub database_warmup_ratio: usize,
    pub max_pull_count: u8,
    pub request_timeout: Duration,
    pub throttle_coefficient: usize,
    pub throttle_wait: Duration,
    pub block_processor_threshold: usize,
    /** Minimum accepted protocol version used when bootstrapping */
    pub min_protocol_version: u8,
    pub max_requests: usize,
    pub optimistic_request_percentage: u8,
    pub bootstrap_queue: BootstrapQueueConfig,
    pub frontier_scan: FrontierScanConfig,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            enable: true,
            enable_block_requester: true,
            enable_dependency_walker: true,
            enable_frontier_scan: true,
            channel_limit: 16,
            rate_limit: 500,
            database_rate_limit: 256,
            frontier_rate_limit: 8,
            database_warmup_ratio: 10,
            max_pull_count: BlocksAckPayload::MAX_BLOCKS,
            request_timeout: Duration::from_secs(15),
            throttle_coefficient: 8 * 1024,
            throttle_wait: Duration::from_millis(100),
            block_processor_threshold: 1024 * 8,
            min_protocol_version: 0x14, // TODO don't hard code
            max_requests: 1024,
            optimistic_request_percentage: 75,
            bootstrap_queue: Default::default(),
            frontier_scan: Default::default(),
        }
    }
}

pub struct Bootstrapper {
    stats: Arc<Stats>,
    cleanup_thread: Mutex<Option<JoinHandle<()>>>,
    query_tracker: Arc<QueryTracker>,
    bootstrap_queue: Arc<BootstrapQueue>,
    config: BootstrapConfig,
    clock: Arc<SteadyClock>,
    response_handler: ResponseProcessor,
    block_inspector: BlockInspector,
    requesters: Requesters,
    ledger: Arc<Ledger>,
    frontier_scan: Arc<FrontierScan>,
    stopped: Arc<NullableCondvarMutex<StoppedFlag>>,
}

impl Bootstrapper {
    pub fn new(
        block_processor_queue: Arc<BlockProcessorQueue>,
        ledger: Arc<Ledger>,
        stats: Arc<Stats>,
        network: Arc<RwLock<Network>>,
        message_sender: MessageSender,
        config: BootstrapConfig,
    ) -> Self {
        let bootstrap_queue = Arc::new(BootstrapQueue::new(config.bootstrap_queue.clone()));
        let frontier_scan = Arc::new(FrontierScan::new(
            stats.clone(),
            ledger.clone(),
            bootstrap_queue.clone(),
            config.frontier_scan.clone(),
        ));
        let stopped = NullableCondvarMutex::new(StoppedFlag::default());
        Self::new_impl(
            block_processor_queue,
            ledger,
            stats,
            network,
            frontier_scan,
            bootstrap_queue,
            message_sender,
            config,
            Arc::new(SteadyClock::default()),
            stopped,
        )
    }

    #[cfg(test)]
    pub fn new_null() -> Self {
        let block_processor_queue = Arc::new(BlockProcessorQueue::default());
        let ledger = Arc::new(Ledger::new_null());
        let stats = Arc::new(Stats::default());
        let network = Arc::new(RwLock::new(Network::new_test_instance()));
        let message_sender = MessageSender::new_null();
        let config = BootstrapConfig::default();
        let clock = Arc::new(SteadyClock::new_null());
        let frontier_scan = Arc::new(FrontierScan::new_null());
        let bootstrap_queue = Arc::new(BootstrapQueue::new_null());
        let stopped = NullableCondvarMutex::new_null(StoppedFlag::default());

        Self::new_impl(
            block_processor_queue,
            ledger,
            stats,
            network,
            frontier_scan,
            bootstrap_queue,
            message_sender,
            config,
            clock,
            stopped,
        )
    }

    fn new_impl(
        block_processor_queue: Arc<BlockProcessorQueue>,
        ledger: Arc<Ledger>,
        stats: Arc<Stats>,
        network: Arc<RwLock<Network>>,
        frontier_scan: Arc<FrontierScan>,
        bootstrap_queue: Arc<BootstrapQueue>,
        message_sender: MessageSender,
        config: BootstrapConfig,
        clock: Arc<SteadyClock>,
        stopped: NullableCondvarMutex<StoppedFlag>,
    ) -> Self {
        let query_tracker = Arc::new(QueryTracker::new(config.clone(), stats.clone()));

        let response_handler = ResponseProcessor::new(
            query_tracker.clone(),
            bootstrap_queue.clone(),
            block_processor_queue.clone(),
            frontier_scan.clone(),
        );

        let block_inspector = BlockInspector::new(
            bootstrap_queue.clone(),
            ledger.clone(),
            block_processor_queue.clone(),
        );

        let requesters = Requesters::new(
            config.clone(),
            stats.clone(),
            message_sender.clone(),
            query_tracker.clone(),
            ledger.clone(),
            block_processor_queue,
            bootstrap_queue.clone(),
            network,
            frontier_scan.clone(),
        );

        Self {
            cleanup_thread: Mutex::new(None),
            query_tracker,
            config,
            stats,
            clock,
            response_handler,
            block_inspector,
            requesters,
            ledger,
            bootstrap_queue,
            frontier_scan,
            stopped: stopped.into(),
        }
    }

    pub fn stop(&self) {
        self.stopped.lock().stopped = true;
        self.stopped.notify_all();

        self.requesters.stop();

        let join_handle = self.cleanup_thread.lock().unwrap().take();
        if let Some(handle) = join_handle {
            handle.join().unwrap();
        }
    }

    pub fn contains(&self, account: &Account) -> bool {
        self.bootstrap_queue.contains(account)
    }

    pub fn enqueue(&self, account: Account) {
        self.bootstrap_queue.enqueue(account);
    }

    pub fn enqueue_batch(&self, accounts: impl IntoIterator<Item = Account>) {
        for account in accounts {
            self.bootstrap_queue.enqueue(account);
        }
    }

    pub fn clear_blocked_accounts(&self) {
        self.bootstrap_queue.clear_blocked_accounts();
    }

    pub fn verify_blocked_accounts(&self) {
        tracing::info!("Verifying blocked accounts...");
        let missing_sends = self.bootstrap_queue.missing_sends();
        let any = self.ledger.any();
        for block_hash in &missing_sends {
            if any.block_exists(block_hash) {
                tracing::warn!(
                    "Found send block that is still marked as missing: {}",
                    block_hash
                );
            }
        }
        tracing::info!("Blocked accounts verfied!")
    }

    pub fn print_processing(&self) {
        let processing = self.bootstrap_queue.processing();
        tracing::info!("Processing blocks:");
        for hash in processing {
            tracing::info!("Processing: {}", hash);
        }
        tracing::info!("Processing blocks end");
    }

    /// Process `asc_pull_ack` message coming from network
    pub fn process(&self, message: AscPullAck, channel_id: ChannelId) {
        let now = self.clock.now();
        let query_id = message.id;
        let result = self.response_handler.process(message, channel_id, now);
        match result {
            Ok(info) => {
                trace!(query_id, ?channel_id, "Response processed");
                self.stats.inc(StatType::Bootstrap, DetailType::Reply);
                self.stats
                    .inc(StatType::BootstrapReply, info.query_type.into());
                self.stats.sample(
                    Sample::BootstrapTagDuration,
                    info.response_time.as_millis() as i64,
                    (0, self.config.request_timeout.as_millis() as i64),
                );
            }
            Err(error) => {
                trace!(query_id, ?channel_id, ?error, "Response processing failed");
                match error {
                    ProcessError::NoRunningQueryFound => {
                        self.stats.inc(StatType::Bootstrap, DetailType::MissingTag);
                    }
                    ProcessError::InvalidResponseType => {
                        self.stats
                            .inc(StatType::Bootstrap, DetailType::InvalidResponseType);
                    }
                    ProcessError::InvalidResponse => {
                        self.stats
                            .inc(StatType::Bootstrap, DetailType::InvalidResponse);
                    }
                }
            }
        }
    }

    fn inspect_blocks(&self, batch: &[ProcessResult]) {
        self.block_inspector.inspect(batch);
    }

    fn unblock_batch(&self, accounts: impl IntoIterator<Item = Account>) {
        for account in accounts {
            self.bootstrap_queue.unblock(account);
        }
    }

    fn run_timeouts(&self) {
        let mut stopped = self.stopped.lock();
        let mut last_sync = self.clock.now();
        while !stopped.stopped {
            self.query_tracker.timeout();
            self.bootstrap_queue.timeout();

            if last_sync.elapsed(self.clock.now()) >= Duration::from_mins(1) {
                self.bootstrap_queue.sync_dependencies();
                last_sync = self.clock.now();
            }

            self.stopped.notify_all();

            stopped = self
                .stopped
                .wait_timeout_while(stopped, Duration::from_secs(1), |s| !s.stopped)
                .0;
        }
    }

    pub fn queue_info(&self) -> BootstrapQueueInfo {
        self.bootstrap_queue.info()
    }

    pub fn queue_snapshot(&self, limit: usize, filter: Option<Account>) -> BootstrapQueueSnapshot {
        self.bootstrap_queue.snapshot(limit, filter)
    }

    pub fn frontier_scan_snapshot(&self) -> FrontierScanSnapshot {
        self.frontier_scan.snapshot()
    }
}

impl Drop for Bootstrapper {
    fn drop(&mut self) {
        // All threads must be stopped before destruction
        debug_assert!(self.cleanup_thread.lock().unwrap().is_none());
    }
}

impl ContainerInfoProvider for Bootstrapper {
    fn container_info(&self) -> ContainerInfo {
        ContainerInfo::builder()
            .node("query_tracker", self.query_tracker.container_info())
            .node("bootstrap_queue", self.bootstrap_queue.container_info())
            .node("frontiers", self.frontier_scan.container_info())
            .finish()
    }
}

impl StatsSource for Bootstrapper {
    fn collect_stats(&self, result: &mut StatsCollection) {
        self.response_handler.collect_stats(result);
        self.bootstrap_queue.collect_stats(result);
        self.requesters.collect_stats(result);
        self.frontier_scan.collect_stats(result);
    }
}

pub trait BootstrapExt {
    fn start(&self);
}

impl BootstrapExt for Arc<Bootstrapper> {
    fn start(&self) {
        debug_assert!(self.cleanup_thread.lock().unwrap().is_none());

        if !self.config.enable {
            warn!("Ascending bootstrap is disabled");
            return;
        }

        self.requesters.start();

        let self_l = Arc::clone(self);
        let cleanup = std::thread::Builder::new()
            .name("Bootstrap clean".to_string())
            .spawn(Box::new(move || self_l.run_timeouts()))
            .unwrap();

        *self.cleanup_thread.lock().unwrap() = Some(cleanup);
    }
}

pub(crate) struct BootstrapperCleanup(pub Arc<Bootstrapper>);

impl DeadChannelCleanupStep for BootstrapperCleanup {
    fn clean_up_dead_channels(&self, dead_channel_ids: &[ChannelId]) {
        self.0
            .query_tracker
            .clean_up_dead_channels(dead_channel_ids);
    }
}

impl EventHandler<LedgerPipelineEvent> for Bootstrapper {
    fn handle(&self, event: &LedgerPipelineEvent) {
        match event {
            LedgerPipelineEvent::Ledger(LedgerEvent::BlocksProcessed(results)) => {
                self.inspect_blocks(&results);
            }
            LedgerPipelineEvent::Ledger(LedgerEvent::BlocksRolledBack(rolled_back)) => {
                self.unblock_batch(rolled_back.affected_accounts());
            }
            _ => {}
        }
    }
}

pub struct FrontierScanSnapshot {
    pub processed_frontiers: u64,
    pub outdated_accounts_found: u64,
    pub heads: Vec<FrontierHeadInfo>,
    pub last_outdated_accounts: Vec<Account>,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum VerifyResult {
    Ok,
    NothingNew,
    Invalid,
}

#[derive(Default)]
struct StoppedFlag {
    pub stopped: bool,
}
