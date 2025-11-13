use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

#[cfg(unix)]
use std::{
    fs::Permissions,
    os::unix::fs::PermissionsExt,
};

use bounded_vec_deque::BoundedVecDeque;
use num_format::{Locale, ToFormattedString};
use tracing::{error, info, warn};

use rsnano_ledger::{AnySet, BlockError, Ledger, LedgerBuilder, LedgerSet};
use rsnano_messages::NetworkFilter;
use rsnano_network::{
    ChannelId, DeadChannelCleanup, Network, NetworkCleanup, PeerConnector, TcpListener,
    TcpListenerExt, TcpNetworkAdapter, TrafficType,
};
use rsnano_network_protocol::{
    HandshakeStats, InboundMessageQueue, InboundMessageQueueCleanup, LatestKeepalives,
    LatestKeepalivesCleanup, NanoDataReceiverFactory, SynCookies,
};
use rsnano_nullable_clock::{SteadyClock, SystemTimeFactory};
use rsnano_nullable_fs::NullableFilesystem;
use rsnano_nullable_lmdb::{
    EnvironmentFlags, EnvironmentOptions, LmdbEnvironment, LmdbEnvironmentFactory,
};
use rsnano_output_tracker::OutputListenerMt;
use rsnano_types::{
    Account, Amount, Block, BlockHash, Networks, NodeId, Peer, PrivateKey, QualifiedRoot, Root,
    SavedBlock, Vote, VoteError, WorkNonce, WorkRequest,
};
use rsnano_utils::{
    CancellationToken,
    container_info::{ContainerInfo, ContainerInfoFactory, ContainerInfoProvider},
    stats::{Direction, Stats, StatsCollection, StatsCollector},
    sync::backpressure_channel,
    thread_pool::ThreadPool,
    ticker::{Tickable, TickerPool, TimerThread},
};
use rsnano_wallet::{ReceivableSearch, WalletBackup, Wallets, WalletsTicker};

#[cfg(feature = "ledger_snapshots")]
use crate::ledger_snapshots::{LedgerSnapshots, fork_detector::ForkDetector};
use crate::{
    NodeCallbacks, OnlineWeightSampler,
    aec_event_processor::AecEventProcessor,
    block_processing::{
        BacklogScan, BacklogWaiter, BlockContext, BlockProcessor, BlockProcessorQueue, BlockSource,
        BoundedBacklog, BoundedBacklogPlugin, LocalBlockBroadcaster, LocalBlockBroadcasterExt,
        LocalBlockBroadcasterPlugin, ProcessQueueConfig, ProcessedResult, UncheckedBlockReenqueuer,
        UncheckedMap,
    },
    block_rate_calculator::{BlockRateCalculator, CurrentBlockRates},
    bootstrap::{
        BootstrapExt, BootstrapResponderCleanup, BootstrapServer, Bootstrapper, BootstrapperCleanup,
    },
    cementation::{ConfirmingSet, TrackConfirmationTimes},
    config::{GlobalConfig, NetworkParams, NodeConfig, NodeFlags},
    consensus::{
        ActiveElectionsContainer, AecForkInserter, AecTicker, AecVoter, BootstrapElectionActivator,
        BootstrapStaleElections, ConfirmReqSender, ConfirmationSolicitorPlugin, CpsLimiter,
        CurrentRepTiers, DependentElectionsConfirmer, ForkCache, ForkCacheUpdater,
        LocalVoteHistory, LocalVotesRemover, RepTiersCalculator, RequestAggregator,
        RequestAggregatorCleanup, VoteApplier, VoteBroadcaster, VoteCache, VoteCacheProcessor,
        VoteGenerators, VoteProcessor, VoteProcessorExt, VoteProcessorQueue,
        VoteProcessorQueueCleanup, VoteRebroadcastQueue, VoteRebroadcaster, WalletRepsChecker,
        WinnerBlockBroadcaster,
        election::ConfirmedElection,
        election_schedulers::{ElectionSchedulers, ElectionSchedulersPlugin},
        get_bootstrap_weights, log_bootstrap_weights,
    },
    ledger_event_processor::{LedgerEventProcessor, LedgerEventProcessorPlugin},
    node_id_key_file::NodeIdKeyFile,
    node_monitor::NodeMonitor,
    recently_cemented_inserter::RecentlyCementedInserter,
    representatives::{
        OnlineReps, OnlineRepsCleanup, OnlineWeightCalculation, RepCrawler, RepCrawlerExt,
    },
    telemetry::{
        TelementryConfig, TelementryExt, Telemetry, TelemetryFactory, rsnano_build_info,
        rsnano_version_string,
    },
    tokio_runner::TokioRunner,
    transport::{
        MessageFlooder, MessageProcessor, MessageSender, NetworkMessageProcessor, NetworkThreads,
        PeerCacheConnector, PeerCacheUpdater,
        keepalive::{KeepaliveMessageFactory, KeepalivePublisher},
        run_loopback_channel_adapter,
    },
    utils::spawn_backpressure_processor,
    wallets::{
        LocalRepsComputation, WalletRepresentatives, block_processor::WalletBlockProcessor,
        work::WalletWorkProvider,
    },
    work::WorkFactory,
};

#[allow(dead_code)]
pub struct Node {
    is_nulled: bool,
    pub runtime: tokio::runtime::Handle,
    pub data_path: PathBuf,
    pub steady_clock: Arc<SteadyClock>,
    pub node_id: PrivateKey,
    pub config: NodeConfig,
    pub network_params: NetworkParams,
    pub stats: Arc<Stats>,
    workers: Arc<ThreadPool>,
    pub flags: NodeFlags,
    pub work_factory: Arc<WorkFactory>,
    pub unchecked: Arc<Mutex<UncheckedMap>>,
    pub ledger: Arc<Ledger>,
    pub network: Arc<RwLock<Network>>,
    pub telemetry: Arc<Telemetry>,
    pub bootstrap_server: Arc<BootstrapServer>,
    pub online_reps: Arc<Mutex<OnlineReps>>,
    pub rep_tiers: Arc<CurrentRepTiers>,
    pub vote_processor_queue: Arc<VoteProcessorQueue>,
    pub history: Arc<LocalVoteHistory>,
    pub confirming_set: Arc<ConfirmingSet>,
    pub vote_cache: Arc<Mutex<VoteCache>>,
    pub block_processor: Arc<BlockProcessor>,
    pub block_processor_queue: Arc<BlockProcessorQueue>,
    pub wallets: Arc<Wallets>,
    pub vote_generators: Arc<VoteGenerators>,
    pub active: Arc<RwLock<ActiveElectionsContainer>>,
    pub vote_processor: Arc<VoteProcessor>,
    vote_cache_processor: Arc<VoteCacheProcessor>,
    pub rep_crawler: Arc<RepCrawler>,
    pub tcp_listener: Arc<TcpListener>,
    pub election_schedulers: Arc<ElectionSchedulers>,
    pub request_aggregator: Arc<RequestAggregator>,
    pub backlog_scan: BacklogScan,
    bounded_backlog: Arc<BoundedBacklog>,
    pub bootstrapper: Arc<Bootstrapper>,
    pub local_block_broadcaster: Arc<LocalBlockBroadcaster>,
    message_processor: Mutex<MessageProcessor>,
    network_threads: Arc<Mutex<NetworkThreads>>,
    pub peer_connector: Arc<PeerConnector>,
    pub inbound_message_queue: Arc<InboundMessageQueue>,
    stopped: AtomicBool,
    pub network_filter: Arc<NetworkFilter>,
    pub message_sender: Arc<Mutex<MessageSender>>, // TODO remove this. It is needed right now
    pub message_flooder: Arc<Mutex<MessageFlooder>>, // TODO remove this. It is needed right now
    pub keepalive_publisher: Arc<KeepalivePublisher>,
    start_stop_listener: OutputListenerMt<&'static str>,
    vote_rebroadcaster: VoteRebroadcaster,
    tokio_runner: TokioRunner,
    pub aec_ticker: TimerThread<AecTicker>,
    pub recently_cemented: Arc<Mutex<BoundedVecDeque<ConfirmedElection>>>,
    pub stats_collector: StatsCollector,
    container_info_factory: ContainerInfoFactory,
    winner_block_broadcaster: Arc<Mutex<WinnerBlockBroadcaster>>,
    pub block_rates: Arc<CurrentBlockRates>,
    aec_voter: TimerThread<AecVoter>,
    pub wallet_reps: Arc<Mutex<WalletRepresentatives>>,
    ticker_pool: TickerPool,
    #[cfg(feature = "ledger_snapshots")]
    pub ledger_snapshots: Arc<LedgerSnapshots>,
}

pub(crate) struct NodeArgs {
    pub data_path: PathBuf,
    pub config: NodeConfig,
    pub network_params: NetworkParams,
    pub flags: NodeFlags,
    pub callbacks: NodeCallbacks,
    pub event_sender: Option<SyncSender<NodeEvent>>,
}

impl NodeArgs {
    pub fn create_test_instance() -> Self {
        let network_params = NetworkParams::new(Networks::NanoLiveNetwork);
        let config = NodeConfig::new(None, &network_params, 2);
        Self {
            data_path: "/home/nulled-node".into(),
            network_params,
            config,
            flags: Default::default(),
            callbacks: Default::default(),
            event_sender: None,
        }
    }
}

impl Node {
    pub fn new_null() -> Self {
        Self::new_null_with_callbacks(Default::default())
    }

    pub fn new_null_with_callbacks(callbacks: NodeCallbacks) -> Self {
        let args = NodeArgs {
            callbacks,
            ..NodeArgs::create_test_instance()
        };
        Self::new(args, true, NodeIdKeyFile::new_null())
    }

    pub(crate) fn new_with_args(args: NodeArgs) -> Self {
        Self::new(args, false, NodeIdKeyFile::default())
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id.public_key().into()
    }

    fn new(args: NodeArgs, is_nulled: bool, mut node_id_key_file: NodeIdKeyFile) -> Self {
        let mut tokio_runner = TokioRunner::new(args.config.io_threads);
        tokio_runner.start();
        let runtime = tokio_runner.handle().clone();

        let network_params = args.network_params;
        let current_network = network_params.network.current_network;
        let network_label = network_params.network.get_current_network_as_string();
        let application_path = args.data_path;

        info!("Node started");
        info!("Version: {}", rsnano_version_string());
        info!("{}", rsnano_build_info());
        info!("Network: {}", network_label);
        info!("Data path: {:?}", application_path);
        info!(
            "Genesis block: {}",
            network_params.ledger.genesis_block.hash()
        );
        info!(
            "Genesis account: {}",
            network_params.ledger.genesis_account.encode_account()
        );

        let mut config = args.config;
        let flags = args.flags;
        if flags.enable_voting {
            config.enable_voting = true;
        }

        let work_factory = Arc::new(
            WorkFactory::builder(runtime.clone())
                .local_work_pool(|p| {
                    p.threads(config.work_threads as usize)
                        .cpu_rate_limit(Duration::from_millis(config.pow_sleep_interval_ns as u64))
                        .opencl_config(config.opencl.clone())
                        .enable_gpu(config.enable_opencl)
                })
                .work_peers(config.work_peers.clone())
                .finish(),
        );
        info!(
            "Work pool threads: {} ({})",
            work_factory.work_threads(),
            if work_factory.has_opencl() {
                "OpenCL"
            } else {
                "CPU"
            }
        );
        info!("Work peers: {}", config.work_peers.len());

        let node_observer = args.event_sender;
        // Time relative to the start of the node. This makes time exlpicit and enables us to
        // write time relevant unit tests with ease.
        let steady_clock = if is_nulled {
            Arc::new(SteadyClock::new_null())
        } else {
            Arc::new(SteadyClock::default())
        };

        let global_config = &GlobalConfig {
            node_config: config.clone(),
            flags: flags.clone(),
            network_params: network_params.clone(),
        };
        let node_id_key = node_id_key_file.initialize(&application_path).unwrap();
        let node_id = NodeId::from(&node_id_key);
        info!("Node ID: {}", node_id);

        let stats = Arc::new(Stats::new(Default::default()));

        let bootstrap_weights = if (network_params.network.is_live_network()
            || network_params.network.is_beta_network())
            && !flags.inactive_node
        {
            get_bootstrap_weights(current_network)
        } else {
            Default::default()
        };

        let fs = if is_nulled {
            NullableFilesystem::new_null()
        } else {
            NullableFilesystem::default()
        };

        if !fs.exists(&application_path) {
            fs.create_dir_all(&application_path)
                .expect("Could not create data dir");
            #[cfg(unix)]
            fs.set_permissions(&application_path, Permissions::from_mode(0o700))
                .expect("Could not set data dir permissions");
        }

        let mut ledger_path = application_path.clone();
        ledger_path.push("data.ldb");

        let lmdb_env_factory = if is_nulled {
            LmdbEnvironmentFactory::new_null()
        } else {
            LmdbEnvironmentFactory::default()
        };

        info!("LMDB sync strategy: {:?}", config.lmdb_config.sync);
        info!("Loading ledger, this may take a while...");
        let ledger = LedgerBuilder::new(&ledger_path)
            .env_factory(&lmdb_env_factory)
            .config(config.lmdb_config.clone())
            .constants(network_params.ledger.clone())
            .min_rep_weight(config.representative_vote_weight_minimum)
            .bootstrap_weights(bootstrap_weights)
            .stats(stats.clone())
            .finish();

        let ledger = match ledger {
            Ok(i) => i,
            Err(e) => {
                panic!("Could not open ledger: {:?}. Details: {:?}", ledger_path, e)
            }
        };

        // hard coded version! TODO: read version from Cargo
        info!("Database backend: {}", ledger.store_vendor());

        let rep_weights = ledger.rep_weights.clone();

        let mut event_queues_info = ContainerInfoFactory::new();
        let (ledger_tx, ledger_rx) = backpressure_channel::channel(1024);
        let ledger_tx_clone = ledger_tx.clone();
        event_queues_info.add_leaf("ledger", move || ledger_tx_clone.len());

        let ledger = Arc::new(ledger);
        info!(
            "Block count:     {}",
            ledger.block_count().to_formatted_string(&Locale::en)
        );
        info!(
            "Confirmed count: {}",
            ledger.confirmed_count().to_formatted_string(&Locale::en)
        );
        info!(
            "Account count:   {}",
            ledger.account_count().to_formatted_string(&Locale::en)
        );
        info!(
            "Representative count: {}",
            rep_weights.len().to_formatted_string(&Locale::en)
        );

        log_bootstrap_weights(&rep_weights);

        let mut ledger_event_processor_plugins: Vec<Box<dyn LedgerEventProcessorPlugin>> =
            Vec::new();

        let syn_cookies = Arc::new(SynCookies::new(network_params.network.max_peers_per_ip));

        let workers = Arc::new(ThreadPool::new(
            config.background_threads as usize,
            "Worker".to_string(),
        ));
        let mut ticker_pool = TickerPool::with_thread_pool(workers.clone());

        let mut inbound_message_queue =
            InboundMessageQueue::new(config.message_processor.max_queue);
        if let Some(cb) = args.callbacks.on_inbound {
            inbound_message_queue.set_inbound_callback(cb);
        }
        if let Some(cb) = args.callbacks.on_inbound_dropped {
            inbound_message_queue.set_inbound_dropped_callback(cb);
        }
        let inbound_message_queue = Arc::new(inbound_message_queue);

        let network = Network::new(config.network.clone());
        runtime.spawn(run_loopback_channel_adapter(
            network.loopback().clone(),
            node_id,
            current_network,
            inbound_message_queue.clone(),
        ));
        let network = Arc::new(RwLock::new(network));

        let mut network_filter = NetworkFilter::new(config.network_duplicate_filter_size);
        network_filter.age_cutoff = config.network_duplicate_filter_cutoff;
        let network_filter = Arc::new(network_filter);

        let unchecked = Arc::new(Mutex::new(UncheckedMap::new(
            config.max_unchecked_blocks as usize,
        )));

        let online_reps = Arc::new(Mutex::new(
            OnlineReps::builder()
                .rep_weights(rep_weights.clone())
                .online_weight_minimum(config.online_weight_minimum)
                .representative_weight_minimum(config.representative_vote_weight_minimum)
                .weight_interval(OnlineReps::default_interval_for(current_network))
                .finish(),
        ));

        let online_weight_sampler =
            OnlineWeightSampler::new(ledger.clone(), network_params.network.current_network);

        let mut online_weight_calculation = OnlineWeightCalculation::new(
            online_weight_sampler,
            online_reps.clone(),
            steady_clock.clone(),
        );
        // Make sure that online weight is properly calculated from the beginning;
        online_weight_calculation.tick(&CancellationToken::new());
        ticker_pool.insert(
            online_weight_calculation,
            OnlineReps::default_interval_for(current_network),
        );

        let mut message_sender =
            MessageSender::new(stats.clone(), network_params.network.protocol_info());

        if let Some(callback) = &args.callbacks.on_publish {
            message_sender.set_published_callback(callback.clone());
        }

        let message_flooder = MessageFlooder::new(
            online_reps.clone(),
            network.clone(),
            stats.clone(),
            message_sender.clone(),
        );

        let telemetry_config = TelementryConfig {
            enable_ongoing_broadcasts: !flags.disable_providing_telemetry_metrics,
        };
        let telemetry_factory = TelemetryFactory {
            ledger: ledger.clone(),
            network: network.clone(),
            node_id_key: node_id_key.clone(),
            unchecked: unchecked.clone(),
            startup_time: steady_clock.now(),
            clock: steady_clock.clone(),
        };
        let telemetry = Arc::new(Telemetry::new(
            telemetry_factory,
            telemetry_config,
            stats.clone(),
            ledger.genesis().hash(),
            network_params.clone(),
            network.clone(),
            message_sender.clone(),
            steady_clock.clone(),
        ));

        let bootstrap_server = Arc::new(BootstrapServer::new(
            config.bootstrap_server.clone(),
            stats.clone(),
            ledger.clone(),
            steady_clock.clone(),
            message_sender.clone(),
        ));

        let vote_processor_queue = Arc::new(VoteProcessorQueue::new(
            config.vote_processor.clone(),
            stats.clone(),
        ));

        let vote_history = Arc::new(LocalVoteHistory::new(
            network_params.network.current_network,
        ));

        let confirming_set = Arc::new(ConfirmingSet::new(
            config.confirming_set.clone(),
            ledger.clone(),
            stats.clone(),
        ));
        confirming_set.set_event_publisher(ledger_tx.clone());

        let vote_cache = Arc::new(Mutex::new(VoteCache::new(
            config.vote_cache.clone(),
            stats.clone(),
        )));

        let fork_cache = Arc::new(RwLock::new(ForkCache::with(
            config.fork_cache_max_size,
            config.fork_cache_max_forks_per_root,
        )));

        let block_processor_config = ProcessQueueConfig::from(global_config);
        let block_processor_queue = Arc::new(BlockProcessorQueue::new(block_processor_config));

        let unchecked_reenqueuer = UncheckedBlockReenqueuer::new(
            unchecked.clone(),
            ledger.clone(),
            block_processor_queue.clone(),
            steady_clock.clone(),
        );
        ticker_pool.insert(unchecked_reenqueuer.clone(), Duration::from_secs(1));

        let mut wallets_path = application_path.clone();
        wallets_path.push("wallets.ldb");

        let wallets_env = if is_nulled {
            Arc::new(LmdbEnvironment::new_null())
        } else {
            let options = EnvironmentOptions {
                path: wallets_path,
                max_dbs: 128,
                map_size: 1024 * 1024 * 1024,
                flags: EnvironmentFlags::NO_SUB_DIR
                    | EnvironmentFlags::NO_TLS
                    | EnvironmentFlags::NO_READAHEAD,
            };
            Arc::new(
                lmdb_env_factory
                    .create(options)
                    .expect("Could not create LMDB env for wallets"),
            )
        };

        let wallets_config = global_config.wallets_config();

        let mut wallets = Wallets::new(
            wallets_config.clone(),
            wallets_env,
            ledger.clone(),
            network_params.work.clone(),
            steady_clock.clone(),
        );
        if !is_nulled {
            wallets.initialize().expect("Could not create wallet");
        }

        let wallets = Arc::new(wallets);

        let (tx_work, rx_work) = mpsc::channel();
        wallets.set_work_queue(tx_work);

        let (tx_block, rx_block) = mpsc::channel();
        wallets.set_block_queue(tx_block);

        let wallet_work = WalletWorkProvider::new(wallets.clone(), rx_work, work_factory.clone());

        std::thread::Builder::new()
            .name("Wallet work".to_owned())
            .spawn(move || {
                wallet_work.run();
            })
            .unwrap();

        let wallet_blocks =
            WalletBlockProcessor::new(rx_block, wallets.clone(), block_processor_queue.clone());

        std::thread::Builder::new()
            .name("Wallet blocks".to_owned())
            .spawn(move || wallet_blocks.run())
            .unwrap();

        let wallet_reps = Arc::new(Mutex::new(WalletRepresentatives::new(
            wallets_config.voting_enabled,
            wallets_config.vote_minimum,
            ledger.rep_weights.clone(),
            wallets.clone(),
            online_reps.clone(),
        )));
        wallet_reps.lock().unwrap().compute_reps();

        let vote_broadcaster = Arc::new(VoteBroadcaster::new(
            vote_processor_queue.clone(),
            message_flooder.clone(),
            stats.clone(),
        ));

        let vote_generators = Arc::new(VoteGenerators::new(
            ledger.clone(),
            wallet_reps.clone(),
            vote_history.clone(),
            stats.clone(),
            &config,
            &network_params,
            vote_broadcaster,
            message_sender.clone(),
            steady_clock.clone(),
        ));

        let base_latency = match current_network {
            Networks::NanoDevNetwork => Duration::from_millis(25),
            _ => Duration::from_millis(1000),
        };

        let (aec_sender, aec_receiver) = backpressure_channel::channel(1024 * 5);
        let aec_sender_clone = aec_sender.clone();
        event_queues_info.add_leaf("aec", move || aec_sender_clone.len());

        let mut active_elections =
            ActiveElectionsContainer::new(config.active_elections.clone(), base_latency);
        active_elections.set_observer(aec_sender.clone());
        let active_elections = Arc::new(RwLock::new(active_elections));

        let block_rate_calculator = BlockRateCalculator::new(steady_clock.clone(), ledger.clone());
        let block_rates = block_rate_calculator.rates().clone();
        ticker_pool.insert(block_rate_calculator, Duration::from_millis(500));
        let cps_limiter = if config.cps_limit > 0 {
            info!(
                "Confirmations per second (CPS) is limited to: {}",
                config.cps_limit
            );
            CpsLimiter::new(block_rates.clone(), config.cps_limit as usize)
        } else {
            info!("Unlimited confirmations per second (CPS)!");
            CpsLimiter::unlimited()
        };

        let vote_applier = VoteApplier::new(
            active_elections.clone(),
            online_reps.clone(),
            steady_clock.clone(),
            rep_weights.clone(),
            current_network == Networks::NanoDevNetwork,
        );

        let vote_processor = Arc::new(VoteProcessor::new(
            vote_processor_queue.clone(),
            vote_applier,
            stats.clone(),
        ));

        let vote_cache_processor = Arc::new(VoteCacheProcessor::new(
            stats.clone(),
            vote_cache.clone(),
            vote_processor_queue.clone(),
            config.vote_processor.clone(),
        ));

        let recently_cemented = Arc::new(Mutex::new(BoundedVecDeque::new(
            config.confirmation_history_size,
        )));

        let winner_block_broadcaster = Arc::new(Mutex::new(WinnerBlockBroadcaster::new(
            steady_clock.clone(),
            current_network,
            message_flooder.clone(),
            online_reps.clone(),
            network.clone(),
        )));

        let confirm_req_sender = ConfirmReqSender::new(stats.clone(), steady_clock.clone());

        let election_schedulers = Arc::new(ElectionSchedulers::new(
            config.clone(),
            network_params.network.clone(),
            active_elections.clone(),
            ledger.clone(),
            stats.clone(),
            vote_cache.clone(),
            confirming_set.clone(),
            online_reps.clone(),
            steady_clock.clone(),
        ));
        ledger_event_processor_plugins.push(Box::new(ElectionSchedulersPlugin::new(
            election_schedulers.clone(),
        )));

        let mut bootstrap_sender = MessageSender::new_with_buffer_size(
            stats.clone(),
            network_params.network.protocol_info(),
            512,
        );

        if let Some(callback) = &args.callbacks.on_publish {
            bootstrap_sender.set_published_callback(callback.clone());
        }

        let latest_keepalives = Arc::new(Mutex::new(LatestKeepalives::default()));
        let handshake_stats = Arc::new(HandshakeStats::default());

        let inbound_queue_clone = inbound_message_queue.clone();
        let try_enqueue = Arc::new(move |msg, channel| inbound_queue_clone.put(msg, channel));

        let data_receiver_factory = Box::new(NanoDataReceiverFactory::new(
            &network,
            try_enqueue,
            network_filter.clone(),
            stats.clone(),
            handshake_stats.clone(),
            syn_cookies.clone(),
            node_id_key.clone(),
            latest_keepalives.clone(),
            network_params.ledger.genesis_block.hash(),
            network_params.network.protocol_info(),
        ));

        network
            .write()
            .unwrap()
            .set_data_receiver_factory(data_receiver_factory);

        let network_adapter = Arc::new(TcpNetworkAdapter::new(
            network.clone(),
            steady_clock.clone(),
            runtime.clone(),
        ));

        let peer_connector = Arc::new(PeerConnector::new(
            config.tcp.connect_timeout,
            network_adapter.clone(),
            runtime.clone(),
        ));

        let keepalive_factory = Arc::new(KeepaliveMessageFactory::new(
            network.clone(),
            Peer::new(config.external_address.clone(), config.external_port),
        ));

        let keepalive_publisher = Arc::new(KeepalivePublisher::new(
            network.clone(),
            peer_connector.clone(),
            message_sender.clone(),
            keepalive_factory.clone(),
        ));

        let rep_crawler = Arc::new(RepCrawler::new(
            online_reps.clone(),
            stats.clone(),
            config.rep_crawler_query_timeout,
            config.clone(),
            network_params.clone(),
            network.clone(),
            ledger.clone(),
            steady_clock.clone(),
            message_sender.clone(),
            keepalive_publisher.clone(),
            active_elections.clone(),
            runtime.clone(),
        ));

        // BEWARE: `bootstrap` takes `network.port` instead of `config.peering_port` because when the user doesn't specify
        //         a peering port and wants the OS to pick one, the picking happens when `network` gets initialized
        //         (if UDP is active, otherwise it happens when `bootstrap` gets initialized), so then for TCP traffic
        //         we want to tell `bootstrap` to use the already picked port instead of itself picking a different one.
        //         Thus, be very careful if you change the order: if `bootstrap` gets constructed before `network`,
        //         the latter would inherit the port from the former (if TCP is active, otherwise `network` picks first)
        //
        let tcp_listener = Arc::new(TcpListener::new(
            network.read().unwrap().listening_port(),
            network_adapter.clone(),
            runtime.clone(),
        ));

        let request_aggregator = Arc::new(RequestAggregator::new(
            config.request_aggregator.clone(),
            stats.clone(),
            vote_generators.clone(),
            ledger.clone(),
        ));

        let mut backlog_scan =
            BacklogScan::new(global_config.into(), ledger.clone(), steady_clock.clone());

        //  TODO: Hook this direclty in the schedulers
        let schedulers_w = Arc::downgrade(&election_schedulers);
        let ledger_l = ledger.clone();
        backlog_scan.on_unconfirmed_found(move |batch| {
            if let Some(schedulers) = schedulers_w.upgrade() {
                let any = ledger_l.any();
                for info in batch {
                    schedulers.activate_backlog(
                        &any,
                        &info.account,
                        &info.account_info,
                        &info.conf_info,
                    );
                }
            }
        });

        if config.bounded_backlog.max_backlog == 0 {
            config.enable_bounded_backlog = false;
        }
        if !config.enable_bounded_backlog {
            config.bounded_backlog.max_backlog = 0;
        }

        let bounded_backlog = Arc::new(BoundedBacklog::new(
            config.bounded_backlog.clone(),
            ledger.clone(),
            stats.clone(),
            steady_clock.clone(),
            ledger_tx.clone(),
        ));

        if config.enable_bounded_backlog {
            info!(
                "Bounded backlog enabled: max backlog={}, batch_size={}, scan_rate={}",
                config.bounded_backlog.max_backlog,
                config.bounded_backlog.batch_size,
                config.bounded_backlog.scan_rate
            );

            ledger_event_processor_plugins
                .push(Box::new(BoundedBacklogPlugin::new(bounded_backlog.clone())));

            // Activate accounts with unconfirmed blocks
            let backlog_w = Arc::downgrade(&bounded_backlog);
            backlog_scan.on_unconfirmed_found(move |batch| {
                if let Some(backlog) = backlog_w.upgrade() {
                    backlog.activate_batch(batch);
                }
            });

            // Erase accounts with all confirmed blocks
            let backlog_w = Arc::downgrade(&bounded_backlog);
            backlog_scan.on_up_to_date(move |batch| {
                if let Some(backlog) = backlog_w.upgrade() {
                    backlog.erase_accounts(batch);
                }
            });
        }

        let track_conf_times = Box::new(TrackConfirmationTimes::default());
        let conf_time_stats = track_conf_times.stats();
        ledger_event_processor_plugins.push(track_conf_times);

        let bootstrapper = Arc::new(Bootstrapper::new(
            block_processor_queue.clone(),
            ledger.clone(),
            stats.clone(),
            network.clone(),
            message_sender.clone(),
            global_config.node_config.bootstrap.clone(),
            steady_clock.clone(),
        ));
        bootstrapper.initialize(&network_params.ledger.genesis_account);

        let mut aec_ticker = AecTicker::new(active_elections.clone(), steady_clock.clone());

        aec_ticker.add_plugin(ConfirmationSolicitorPlugin {
            message_flooder: message_flooder.clone(),
            online_reps: online_reps.clone(),
            winner_block_broadcaster: winner_block_broadcaster.clone(),
            confirm_req_sender,
        });

        let mut bootstrap_stale =
            BootstrapStaleElections::new(bootstrapper.clone(), steady_clock.clone());
        bootstrap_stale.set_stale_threshold(config.bootstrap_stale_threshold);
        let bootstrap_stale_stats = bootstrap_stale.stats.clone();
        aec_ticker.add_plugin(bootstrap_stale);

        let local_block_broadcaster = Arc::new(LocalBlockBroadcaster::new(
            config.local_block_broadcaster.clone(),
            stats.clone(),
            ledger.clone(),
            confirming_set.clone(),
            steady_clock.clone(),
            message_flooder.clone(),
            !flags.disable_block_processor_republishing,
        ));

        ledger_event_processor_plugins.push(Box::new(LocalBlockBroadcasterPlugin::new(
            local_block_broadcaster.clone(),
        )));

        let vote_cache_w = Arc::downgrade(&vote_cache);
        let active_w = Arc::downgrade(&active_elections);
        let scheduler_w = Arc::downgrade(&election_schedulers);
        let confirming_set_w = Arc::downgrade(&confirming_set);
        let local_block_broadcaster_w = Arc::downgrade(&local_block_broadcaster);

        // TODO: remove the duplication of the on_rolling_back event
        bounded_backlog.can_roll_back(move |hash| {
            if let Some(i) = vote_cache_w.upgrade() {
                if i.lock().unwrap().contains(hash) {
                    return false;
                }
            }

            if let Some(i) = active_w.upgrade() {
                let guard = i.read().unwrap();
                if guard.is_active_hash(hash) || guard.was_recently_confirmed(hash) {
                    return false;
                }
            }

            if let Some(i) = scheduler_w.upgrade() {
                if i.contains(hash) {
                    return false;
                }
            }

            if let Some(i) = confirming_set_w.upgrade() {
                if i.contains(hash) {
                    return false;
                }
            }

            if let Some(i) = local_block_broadcaster_w.upgrade() {
                if i.contains(hash) {
                    return false;
                }
            }
            true
        });

        let backlog_waiter = Arc::new(BacklogWaiter::new(
            block_processor_queue.clone(),
            ledger.clone(),
            steady_clock.clone(),
            config.bounded_backlog.max_backlog,
        ));

        let ledger_tx_clone = ledger_tx.clone();
        let block_processor = Arc::new(BlockProcessor::new(
            block_processor_queue.clone(),
            ledger.clone(),
            unchecked.clone(),
            unchecked_reenqueuer.clone(),
            backlog_waiter.clone(),
            ledger_tx_clone,
            steady_clock.clone(),
        ));

        let mut dead_channel_cleanup = DeadChannelCleanup::new(
            steady_clock.clone(),
            network.clone(),
            network_params.network.cleanup_cutoff(),
        );
        dead_channel_cleanup.add_step(InboundMessageQueueCleanup::new(
            inbound_message_queue.clone(),
        ));

        dead_channel_cleanup.add_step(OnlineRepsCleanup::new(online_reps.clone()));
        dead_channel_cleanup.add_step(BootstrapResponderCleanup::new(
            bootstrap_server.server_impl.clone(),
        ));
        dead_channel_cleanup.add_step(VoteProcessorQueueCleanup::new(vote_processor_queue.clone()));
        dead_channel_cleanup.add_step(block_processor_queue.clone());
        dead_channel_cleanup.add_step(LatestKeepalivesCleanup::new(latest_keepalives.clone()));
        dead_channel_cleanup.add_step(NetworkCleanup::new(network_adapter.clone()));

        dead_channel_cleanup.add_step(RequestAggregatorCleanup::new(
            request_aggregator.state.clone(),
        ));
        dead_channel_cleanup.add_step(BootstrapperCleanup(bootstrapper.clone()));

        #[cfg(feature = "ledger_snapshots")]
        let ledger_snapshots = {
            let wallet_reps2 = wallet_reps.clone();
            Arc::new(LedgerSnapshots::new(
                ledger.clone(),
                move || {
                    // TODO: make this nice:
                    let mut keys = Vec::new();
                    wallet_reps2.lock().unwrap().rep_priv_keys(&mut keys);
                    // For simplicity only take the first key.
                    // TODO: allow multiple keys
                    keys.pop()
                },
                message_flooder.clone(),
                online_reps.clone(),
            ))
        };

        let network_message_processor = Arc::new(NetworkMessageProcessor::new(
            stats.clone(),
            network.clone(),
            network_filter.clone(),
            block_processor_queue.clone(),
            wallet_reps.clone(),
            request_aggregator.clone(),
            vote_processor_queue.clone(),
            telemetry.clone(),
            bootstrap_server.clone(),
            bootstrapper.clone(),
            network_params.work.clone(),
            #[cfg(feature = "ledger_snapshots")]
            ledger_snapshots.clone(),
        ));

        let network_threads = Arc::new(Mutex::new(NetworkThreads::new(
            network.clone(),
            peer_connector.clone(),
            flags.clone(),
            network_params.clone(),
            config.network.clone(),
            stats.clone(),
            syn_cookies.clone(),
            network_filter.clone(),
            keepalive_factory.clone(),
            latest_keepalives.clone(),
            dead_channel_cleanup,
            message_flooder.clone(),
            steady_clock.clone(),
        )));

        let message_processor = Mutex::new(MessageProcessor::new(
            config.clone(),
            inbound_message_queue.clone(),
            network_message_processor.clone(),
        ));

        let rep_crawler_w = Arc::downgrade(&rep_crawler);
        if !flags.disable_rep_crawler {
            network
                .write()
                .unwrap()
                .on_new_realtime_channel(Arc::new(move |channel| {
                    if let Some(crawler) = rep_crawler_w.upgrade() {
                        crawler.query_with_priority(channel);
                    }
                }));
        }

        let vote_rebroadcast_queue = Arc::new(
            VoteRebroadcastQueue::build()
                .max_len(config.vote_rebroadcaster_max_queue)
                .stats(stats.clone())
                .finish(),
        );

        let vote_rebroadcaster = VoteRebroadcaster::new(
            vote_rebroadcast_queue.clone(),
            message_flooder.clone(),
            rep_weights.clone(),
            steady_clock.clone(),
            config.rebroadcast_history.clone(),
        );

        let keepalive_factory_w = Arc::downgrade(&keepalive_factory);
        let message_publisher_l = Arc::new(Mutex::new(message_sender.clone()));
        let message_publisher_w = Arc::downgrade(&message_publisher_l);
        network
            .write()
            .unwrap()
            .on_new_realtime_channel(Arc::new(move |channel| {
                // Send a keepalive message to the new channel
                let Some(factory) = keepalive_factory_w.upgrade() else {
                    return;
                };
                let Some(publisher) = message_publisher_w.upgrade() else {
                    return;
                };
                let keepalive = factory.create_keepalive_self();
                publisher
                    .lock()
                    .unwrap()
                    .try_send(&channel, &keepalive, TrafficType::Keepalive);
            }));

        if !work_factory.work_generation_enabled() {
            info!("Work generation is disabled");
        }

        info!(
            "Outbound bandwidth limit: {} bytes/s, burst ratio: {}",
            config.network.limiter.generic_limit, config.network.limiter.generic_burst_ratio
        );

        let has_local_reps = {
            let reps = wallet_reps.lock().unwrap();
            let has_local_reps = reps.voting_reps() > 0;
            if has_local_reps {
                info!(
                    "Found {} local representatives in wallets",
                    reps.voting_reps()
                );
                for rep in reps.rep_accounts() {
                    info!("Local representative: {}", rep.encode_account());
                }
            }

            has_local_reps
        };

        if has_local_reps {
            if config.enable_voting {
                let voting_reps = wallet_reps.lock().unwrap().voting_reps();
                info!(
                    "Voting is enabled, more system resources will be used, local representatives: {voting_reps}"
                );
                if voting_reps > 1 {
                    warn!("Voting with more than one representative can limit performance");
                }
            } else {
                warn!(
                    "Found local representatives in wallets, but voting is disabled. To enable voting, set `[node] enable_voting=true`n the `config-node.toml` file or use `--enable_voting` command line argument"
                );
            }
        }

        let is_dev_network = network_params.network.is_dev_network();
        let time_factory = SystemTimeFactory::default();

        let peer_cache_updater = PeerCacheUpdater::new(
            network.clone(),
            ledger.clone(),
            time_factory,
            stats.clone(),
            if network_params.network.is_dev_network() {
                Duration::from_secs(10)
            } else {
                Duration::from_secs(60 * 60)
            },
        );
        ticker_pool.insert(
            peer_cache_updater,
            if is_dev_network {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(15)
            },
        );

        let peer_cache_connector = PeerCacheConnector::new(
            ledger.clone(),
            peer_connector.clone(),
            stats.clone(),
            config.network.cached_peer_reachout,
        );
        if !config.network.peer_reachout.is_zero() {
            ticker_pool.insert(peer_cache_connector, config.network.cached_peer_reachout);
        }

        let monitor = NodeMonitor::new(
            ledger.clone(),
            network.clone(),
            online_reps.clone(),
            active_elections.clone(),
            block_rates.clone(),
        );
        if config.enable_monitor {
            ticker_pool.insert(monitor, config.monitor.interval)
        }

        let wallets_ticker = WalletsTicker(wallets.clone());
        ticker_pool.insert(wallets_ticker, Duration::from_millis(500));

        let mut wallet_reps_checker = WalletRepsChecker::new(wallet_reps.clone());
        wallet_reps_checker.add_consumer(vote_rebroadcast_queue.clone());
        ticker_pool.insert(
            wallet_reps_checker,
            if is_dev_network {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(60)
            },
        );

        let rep_tiers = Arc::new(CurrentRepTiers::new());
        let mut rep_tiers_calculator =
            RepTiersCalculator::new(rep_weights.clone(), online_reps.clone(), stats.clone());
        rep_tiers_calculator.add_tiers_consumer(vote_processor_queue.clone());
        rep_tiers_calculator.add_tiers_consumer(vote_rebroadcast_queue.clone());
        rep_tiers_calculator.add_tiers_consumer(rep_tiers.clone());
        ticker_pool.insert(
            rep_tiers_calculator,
            if is_dev_network {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(10)
            },
        );

        let wallet_backup = WalletBackup {
            data_path: application_path.clone(),
            wallets: wallets.clone(),
        };
        if !flags.disable_backup {
            ticker_pool.insert(wallet_backup, Duration::from_secs(60 * 5));
        }

        let receivable_search = ReceivableSearch::new(wallets.clone());
        if !flags.disable_search_pending {
            ticker_pool.insert(
                receivable_search,
                if is_dev_network {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(5)
                },
            );
        }

        let local_reps_computation = LocalRepsComputation::new(wallet_reps.clone());
        ticker_pool.insert(
            local_reps_computation,
            if is_dev_network {
                Duration::from_millis(10)
            } else {
                Duration::from_secs(10)
            },
        );
        let message_flooder = Arc::new(Mutex::new(message_flooder.clone()));

        let recently_cemented_inserter = RecentlyCementedInserter {
            recently_cemented: recently_cemented.clone(),
        };

        let bootstrap_election_activator = BootstrapElectionActivator {
            active_elections: active_elections.clone(),
            vote_cache: vote_cache.clone(),
            stats: stats.clone(),
        };

        let local_votes_remover = LocalVotesRemover {
            active_elections: active_elections.clone(),
            vote_history: vote_history.clone(),
        };

        let aec_fork_inserter = Arc::new(AecForkInserter {
            rep_weights: rep_weights.clone(),
            fork_cache: fork_cache.clone(),
            active_elections: active_elections.clone(),
            vote_cache: vote_cache.clone(),
        });

        let aec_voter = AecVoter::new(
            active_elections.clone(),
            vote_generators.clone(),
            steady_clock.clone(),
            current_network,
            cps_limiter,
        );

        // With ledger_snapshots we never vote for forked blocks!
        #[cfg(not(feature = "ledger_snapshots"))]
        {
            use crate::consensus::ForkInserterPlugin;

            ledger_event_processor_plugins
                .push(Box::new(ForkInserterPlugin::new(aec_fork_inserter.clone())));
        }

        #[cfg(feature = "ledger_snapshots")]
        {
            ledger_event_processor_plugins.push(Box::new(ForkDetector::new(
                ledger.clone(),
                ledger_snapshots.clone(),
                active_elections.clone(),
            )));
        }

        let aec_event_processor = AecEventProcessor {
            vote_cache_processor: vote_cache_processor.clone(),
            node_observer: node_observer.clone(),
            election_schedulers: election_schedulers.clone(),
            network_filter: network_filter.clone(),
            bootstrap_election_activator,
            recently_cemented_inserter,
            vote_cache: vote_cache.clone(),
            vote_rebroadcast_queue: vote_rebroadcast_queue.clone(),
            vote_processor: vote_processor.clone(),
            block_processor_queue: block_processor_queue.clone(),
            confirming_set: confirming_set.clone(),
            online_reps: online_reps.clone(),
            active_elections: active_elections.clone(),
            rep_crawler: rep_crawler.clone(),
            clock: steady_clock.clone(),
            local_votes_remover,
            aec_fork_inserter,
            stats: stats.clone(),
            winner_block_broadcaster: winner_block_broadcaster.clone(),
            plugins: Vec::new(),
        };

        spawn_backpressure_processor("AEC ev proc", aec_receiver, aec_event_processor);

        let dependent_elections_confirmer = DependentElectionsConfirmer {
            confirming_set: confirming_set.clone(),
            active_elections: active_elections.clone(),
            clock: steady_clock.clone(),
        };

        let fork_cache_updater = ForkCacheUpdater::new(fork_cache.clone());

        let ledger_event_processor = LedgerEventProcessor {
            node_event_sender: node_observer.clone(),
            dependent_elections_confirmer,
            confirming_set: confirming_set.clone(),
            stats: stats.clone(),
            bootstrapper: bootstrapper.clone(),
            vote_history: vote_history.clone(),
            active_elections: active_elections.clone(),
            block_processor_queue: block_processor_queue.clone(),
            bounded_backlog: bounded_backlog.clone(),
            fork_cache_updater,
            plugins: ledger_event_processor_plugins,
        };

        spawn_backpressure_processor("Ledger ev proc", ledger_rx, ledger_event_processor);

        vote_processor.add_observer(aec_sender);

        let mut stats_collector = StatsCollector::new();
        stats_collector.add_source(stats.clone());
        stats_collector.add_source(online_reps.clone());
        stats_collector.add_source(fork_cache.clone());
        stats_collector.add_source(active_elections.clone());
        stats_collector.add_source(vote_rebroadcaster.stats.clone());
        stats_collector.add_source(election_schedulers.clone());
        stats_collector.add_source(network.clone());
        stats_collector.add_source(backlog_scan.stats());
        stats_collector.add_source(handshake_stats);
        stats_collector.add_source(inbound_message_queue.clone());
        stats_collector.add_source(bootstrap_stale_stats);
        stats_collector.add_source(block_processor.clone());
        stats_collector.add_source(block_processor_queue.clone());
        stats_collector.add_source(backlog_waiter.clone());
        stats_collector.add_source(conf_time_stats);
        stats_collector.add_source(winner_block_broadcaster.clone());
        stats_collector.add_source(bootstrapper.clone());
        stats_collector.add_source(unchecked.clone());
        stats_collector.add_source(unchecked_reenqueuer.stats().clone());

        let mut container_info = ContainerInfoFactory::new();
        container_info.add("work", work_factory.clone());
        container_info.add("ledger", ledger.clone());
        container_info.add("active", active_elections.clone());
        container_info.add("network", network.clone());
        container_info.add("syn_cookies", syn_cookies);
        container_info.add("telemetry", telemetry.clone());
        container_info.add("wallets", wallets.clone());
        container_info.add("vote_processor", vote_processor_queue.clone());
        container_info.add("vote_cache_processor", vote_cache_processor.clone());
        container_info.add("rep_crawler", rep_crawler.clone());
        container_info.add("block_processor", block_processor_queue.clone());
        container_info.add("online_reps", online_reps.clone());
        container_info.add("history", vote_history.clone());
        container_info.add("confirming_set", confirming_set.clone());
        container_info.add("request_aggregator", request_aggregator.clone());
        container_info.add("election_scheduler", election_schedulers.clone());
        container_info.add("vote_cache", vote_cache.clone());
        container_info.add("vote_generators", vote_generators.clone());
        container_info.add("bootstrapper", bootstrapper.clone());
        container_info.add("unchecked", unchecked.clone());
        container_info.add("local_block_broadcaster", local_block_broadcaster.clone());
        container_info.add("rep_tiers", rep_tiers.clone());
        container_info.add("inbound_msg_queue", inbound_message_queue.clone());
        container_info.add("bounded_backlog", bounded_backlog.clone());
        container_info.add("vote_rebroadcaster", vote_rebroadcast_queue.clone());
        container_info.add("fork_cache", fork_cache.clone());
        container_info.add("event_queues", event_queues_info);

        Self {
            is_nulled,
            steady_clock,
            peer_connector,
            node_id: node_id_key,
            workers,
            work_factory,
            unchecked,
            telemetry,
            network,
            ledger,
            stats,
            data_path: application_path,
            network_params,
            config,
            flags,
            runtime,
            bootstrap_server,
            online_reps,
            rep_tiers,
            vote_processor_queue,
            history: vote_history,
            confirming_set,
            vote_cache,
            block_processor,
            block_processor_queue,
            wallets,
            vote_generators,
            active: active_elections,
            vote_processor,
            vote_cache_processor,
            rep_crawler,
            tcp_listener,
            election_schedulers,
            request_aggregator,
            backlog_scan,
            bounded_backlog,
            bootstrapper,
            local_block_broadcaster,
            network_threads,
            message_processor,
            inbound_message_queue,
            message_sender: message_publisher_l,
            message_flooder,
            network_filter,
            keepalive_publisher,
            stopped: AtomicBool::new(false),
            start_stop_listener: OutputListenerMt::new(),
            vote_rebroadcaster,
            tokio_runner,
            aec_ticker: TimerThread::new("AEC ticker", aec_ticker),
            recently_cemented,
            stats_collector,
            container_info_factory: container_info,
            winner_block_broadcaster,
            block_rates,
            aec_voter: TimerThread::new("AEC voter", aec_voter),
            wallet_reps,
            ticker_pool,
            #[cfg(feature = "ledger_snapshots")]
            ledger_snapshots,
        }
    }

    pub fn container_info(&self) -> ContainerInfo {
        self.container_info_factory.container_info()
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn process_local(&self, block: Block) -> Result<(), BlockError> {
        self.block_processor_queue
            .push_blocking(Arc::new(block), BlockSource::Local)
            .map_err(|_| BlockError::BadSignature)?
            .map(|_| {})
    }

    pub fn try_process(&self, block: Block) -> Result<SavedBlock, BlockError> {
        self.ledger.process_one(&block)
    }

    pub fn process(&self, block: Block) -> SavedBlock {
        let hash = block.hash();
        match self.try_process(block) {
            Ok(saved_block) => saved_block,
            Err(BlockError::Old) | Err(BlockError::Conflict) => self.block(&hash).unwrap(),
            Err(e) => {
                panic!("Could not process block: {:?}", e);
            }
        }
    }

    pub fn process_multi(&self, blocks: &[Block]) {
        for (i, block) in blocks.iter().enumerate() {
            match self.ledger.process_one(block) {
                Ok(_) | Err(BlockError::Old) | Err(BlockError::Conflict) => {}
                Err(e) => {
                    panic!("Could not multi-process block index {}: {:?}", i, e);
                }
            }
        }
    }

    pub fn process_and_confirm_multi(&self, blocks: &[Block]) {
        self.process_multi(blocks);
        self.confirm_multi(blocks);
    }

    pub fn insert_into_wallet(&self, keys: &PrivateKey) {
        let wallet_id = self.wallets.wallet_ids()[0];
        self.wallets
            .insert_adhoc2(&wallet_id, &keys.raw_key(), true)
            .unwrap();
    }

    pub fn process_active(&self, block: Block) {
        self.block_processor_queue.push(BlockContext::new(
            block,
            BlockSource::Live,
            ChannelId::LOOPBACK,
        ));
    }

    pub fn process_local_multi(&self, blocks: &[Block]) {
        for block in blocks {
            let status = self.process_local(block.clone());
            if !matches!(status, Ok(()) | Err(BlockError::Old)) {
                panic!("could not process block!");
            }
        }
    }

    pub fn block(&self, hash: &BlockHash) -> Option<SavedBlock> {
        self.ledger.any().get_block(hash)
    }

    pub fn latest(&self, account: &Account) -> BlockHash {
        self.ledger.any().account_head(account).unwrap_or_default()
    }

    pub fn get_node_id(&self) -> NodeId {
        self.node_id.public_key().into()
    }

    pub fn work_generate_dev(&self, root: impl Into<Root>) -> WorkNonce {
        let difficulty = self.network_params.work.threshold_base();
        self.work_factory
            .generate_work(WorkRequest::new(root.into(), difficulty))
            .unwrap()
    }

    pub fn block_exists(&self, hash: &BlockHash) -> bool {
        self.ledger.any().block_exists(hash)
    }

    pub fn blocks_exist(&self, hashes: &[Block]) -> bool {
        self.block_hashes_exist(hashes.iter().map(|b| b.hash()))
    }

    pub fn block_hashes_exist(&self, hashes: impl IntoIterator<Item = BlockHash>) -> bool {
        let any = self.ledger.any();
        hashes.into_iter().all(|h| any.block_exists(&h))
    }

    pub fn balance(&self, account: &Account) -> Amount {
        self.ledger.any().account_balance(account)
    }

    pub fn confirm_multi(&self, blocks: &[Block]) {
        for block in blocks {
            self.confirm(block.hash());
        }
    }

    pub fn confirm(&self, hash: BlockHash) {
        self.ledger.confirm(hash);
    }

    pub fn block_confirmed(&self, hash: &BlockHash) -> bool {
        self.ledger.confirmed().block_exists(hash)
    }

    pub fn block_hashes_confirmed(&self, blocks: &[BlockHash]) -> bool {
        let confirmed = self.ledger.confirmed();
        blocks.iter().all(|b| confirmed.block_exists(b))
    }

    pub fn blocks_confirmed(&self, blocks: &[Block]) -> bool {
        let confirmed = self.ledger.confirmed();
        blocks.iter().all(|b| confirmed.block_exists(&b.hash()))
    }

    pub fn is_active_root(&self, root: &QualifiedRoot) -> bool {
        self.active.read().unwrap().is_active_root(root)
    }

    pub fn is_active_hash(&self, hash: &BlockHash) -> bool {
        self.active.read().unwrap().is_active_hash(hash)
    }

    pub fn force_confirm(&self, hash: &BlockHash) {
        assert_eq!(
            self.network_params.network.current_network,
            Networks::NanoDevNetwork
        );
        self.active
            .write()
            .unwrap()
            .force_confirm(hash, self.steady_clock.now());
    }

    pub fn get_stat(&self, stat: &'static str, detail: &'static str, dir: Direction) -> u64 {
        self.stats_collector.collect().get_dir(stat, detail, dir)
    }

    pub fn stats(&self) -> MutexGuard<'_, StatsCollection> {
        self.stats_collector.collect()
    }

    /// Note: Start must not be called from an async thread, because it blocks!
    pub fn start(&mut self) {
        self.start_stop_listener.emit("start");
        if self.is_nulled {
            return; // TODO better nullability implementation
        }

        if !self
            .ledger
            .any()
            .block_exists(&self.network_params.ledger.genesis_block.hash())
        {
            error!(
                "Genesis block not found. This commonly indicates a configuration issue, check that the --network or --data_path command line arguments are correct, and also the ledger backend node config option. If using a read-only CLI command a ledger must already exist, start the node with --daemon first."
            );

            if self.network_params.network.is_beta_network() {
                error!("Beta network may have reset, try clearing database files");
            }

            panic!("Genesis block not found!");
        }

        self.network_threads.lock().unwrap().start();
        self.message_processor.lock().unwrap().start();
        self.aec_voter.start(Duration::from_millis(20));

        if !self.flags.disable_rep_crawler {
            self.rep_crawler.start();
        }

        if self.config.tcp.max_inbound_connections > 0 {
            self.tcp_listener.start();
        } else {
            warn!("Peering is disabled");
        }

        if self.config.enable_vote_processor {
            self.vote_processor.start();
        }
        self.vote_cache_processor.start();
        self.block_processor
            .start(self.config.block_processor_threads);
        if !self.flags.disable_request_loop {
            self.aec_ticker
                .start(self.network_params.network.aec_loop_interval);
        }
        self.vote_generators.start();
        self.request_aggregator.start();
        self.confirming_set.start();
        self.election_schedulers.start();
        self.backlog_scan.start();
        if self.config.enable_bounded_backlog {
            self.bounded_backlog.start();
        }
        if self.config.enable_bootstrap_responder {
            self.bootstrap_server.start();
        }
        self.bootstrapper.start();
        self.telemetry.start();
        self.local_block_broadcaster.start();

        if self.config.enable_vote_rebroadcast {
            self.vote_rebroadcaster.start();
        }
        self.ticker_pool.start();
    }

    pub fn stop(&mut self) {
        self.start_stop_listener.emit("stop");
        if self.is_nulled {
            return; // TODO better nullability implementation
        }

        // Ensure stop can only be called once
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        info!("Node stopping...");

        self.ticker_pool.stop();
        self.tcp_listener.stop();
        self.aec_voter.stop();
        self.peer_connector.stop();
        // Cancels ongoing work generation tasks, which may be blocking other threads
        // No tasks may wait for work generation in I/O threads, or termination signal capturing will be unable to call node::stop()
        self.work_factory.stop();
        self.backlog_scan.stop();
        self.bootstrapper.stop();
        self.bounded_backlog.stop();
        self.rep_crawler.stop();
        self.block_processor.stop();
        self.request_aggregator.stop();
        self.vote_cache_processor.stop();
        self.vote_processor.stop();
        self.election_schedulers.stop();
        self.aec_ticker.stop();
        self.active.write().unwrap().stop();
        self.vote_generators.stop();
        self.confirming_set.stop();
        self.telemetry.stop();
        self.bootstrap_server.stop();
        self.wallets.stop();
        self.local_block_broadcaster.stop();
        self.message_processor.lock().unwrap().stop();
        self.network_threads.lock().unwrap().stop(); // Stop network last to avoid killing in-use sockets
        self.vote_rebroadcaster.stop();
        self.workers.join();
        self.tokio_runner.stop();
        // work pool is not stopped on purpose due to testing setup
    }
}

pub enum NodeEvent {
    ElectionStarted(BlockHash),
    ElectionStopped(BlockHash),
    BlockConfirmed(SavedBlock, ConfirmedElection),
    VoteProcessed(Arc<Vote>, Result<(), VoteError>),
    BlocksProcessed(Vec<ProcessedResult>),
}

pub trait NodeEventHandler {
    fn handle(&mut self, event: &NodeEvent);
}

pub struct CompositeNodeEventHandler {
    receiver: Receiver<NodeEvent>,
    handlers: Vec<Box<dyn NodeEventHandler + Send>>,
}
impl CompositeNodeEventHandler {
    pub fn new(receiver: Receiver<NodeEvent>) -> Self {
        Self {
            receiver,
            handlers: Vec::new(),
        }
    }

    pub fn add(&mut self, handler: impl NodeEventHandler + Send + 'static) {
        self.handlers.push(Box::new(handler));
    }

    pub fn run(&mut self) {
        while let Ok(event) = self.receiver.recv() {
            for handler in self.handlers.iter_mut() {
                handler.handle(&event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::{
        AecEvent, AecTickerPlugin, BootstrapStaleElections, StaleElectionsStats,
    };
    use rsnano_utils::{stats::StatsSource, ticker::Tickable};
    use std::any::type_name;

    #[test]
    fn schedule_tickers() {
        let node = Node::new_null();

        assert_ticker::<PeerCacheUpdater>(&node, Duration::from_secs(15));
        assert_ticker::<OnlineWeightCalculation>(&node, Duration::from_secs(20));
        assert_ticker::<RepTiersCalculator>(&node, Duration::from_secs(10));
        assert_ticker::<PeerCacheConnector>(&node, node.config.network.cached_peer_reachout);
        assert_ticker::<NodeMonitor>(&node, node.config.monitor.interval);
        assert_ticker::<WalletBackup>(&node, Duration::from_secs(60 * 5));
        assert_ticker::<ReceivableSearch>(&node, Duration::from_secs(5));
        assert_ticker::<WalletRepsChecker>(&node, Duration::from_secs(60));
        assert_ticker::<BlockRateCalculator>(&node, Duration::from_millis(500));
        assert_ticker::<UncheckedBlockReenqueuer>(&node, Duration::from_secs(1));
        assert_ticker::<LocalRepsComputation>(&node, Duration::from_secs(10));
        assert_ticker::<WalletsTicker>(&node, Duration::from_millis(500));

        // helper:
        fn assert_ticker<T: Tickable + 'static>(node: &Node, expected: Duration) {
            let Some(interval) = node.ticker_pool.get::<T>() else {
                panic!("Should schedule ticker of type: {}", type_name::<T>());
            };
            assert_eq!(interval, expected, "interval for {}", type_name::<T>());
        }
    }

    #[test]
    fn initialize_aec_ticker() {
        let config = NodeConfig {
            bootstrap_stale_threshold: Duration::from_secs(42),
            ..NodeConfig::new_test_instance()
        };
        let args = NodeArgs {
            config: config.clone(),
            ..NodeArgs::create_test_instance()
        };
        let node = Node::new(args, true, NodeIdKeyFile::new_null());
        let task = node.aec_ticker.task();
        let ticker = task.as_ref().unwrap();

        assert_has_aec_ticker_plugin::<ConfirmationSolicitorPlugin>(ticker);

        let stale = assert_has_aec_ticker_plugin::<BootstrapStaleElections>(ticker);
        assert_eq!(
            stale.get_stale_threshold(),
            config.bootstrap_stale_threshold
        );
    }

    fn assert_has_aec_ticker_plugin<T>(ticker: &AecTicker) -> &T
    where
        T: AecTickerPlugin + 'static,
    {
        let plugin = ticker.get_plugin::<T>();
        assert!(
            plugin.is_some(),
            "AEC ticker plugin missing: {}",
            type_name::<T>()
        );
        plugin.unwrap()
    }

    #[test]
    fn initialize_stats_collector() {
        let node = Node::new_null();
        let node_stats = node.stats();
        assert_contains_stats_source(&node_stats, StaleElectionsStats::default());
        assert_contains_stats_source(&node_stats, WinnerBlockBroadcaster::new_null());
    }

    #[test]
    fn connect_winner_block_rebroadcaster() {
        let node = Node::new_null();
        let broadcast_tracker = node.winner_block_broadcaster.lock().unwrap().track();
        let election = ConfirmedElection::new_test_instance();
        let winner_hash = election.winner.hash();

        node.active
            .write()
            .unwrap()
            .simulate_event(AecEvent::ElectionConfirmed(election));

        let output = broadcast_tracker.wait_output().unwrap();
        assert_eq!(output, vec![winner_hash]);
    }

    fn assert_contains_stats_source(node_stats: &StatsCollection, source: impl StatsSource) {
        let mut col = StatsCollection::default();
        source.collect_stats(&mut col);
        let (key, _) = col.iter().next().unwrap().clone();
        assert!(node_stats.contains(key.stat, key.detail, key.dir));
    }
}
