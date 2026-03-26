use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::{Arc, OnceLock, mpsc::SyncSender},
    thread::sleep,
    time::{Duration, Instant},
};

use rsnano_ledger::{
    AnySet, DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH, DEV_GENESIS_PUB_KEY, LedgerSet,
};
use rsnano_network::{Channel, ChannelDirection, NULL_ENDPOINT, TEST_ENDPOINT_1};
use rsnano_node::{
    Node, NodeBuilder, NodeEvent,
    block_processing::BacklogScanConfig,
    config::{NetworkParams, NodeConfig, NodeFlags},
    unique_path,
};
use rsnano_rpc_client::{NanoRpcClient, Url};
use rsnano_rpc_server::run_rpc_server;
use rsnano_types::{
    Account, Amount, Block, BlockHash, DEV_GENESIS_KEY, Epoch, NetworkType, PrivateKey, PublicKey,
    SavedBlock, StateBlockArgs, WalletId,
};

pub struct System {
    pub network_params: NetworkParams,
    pub nodes: Vec<Arc<Node>>,
    pub initialization_blocks: Vec<Block>,
    pub initialization_blocks_cemented: Vec<Block>,
}

impl System {
    pub fn new() -> Self {
        init_tracing();
        let network_params = NetworkParams::new(NetworkType::NanoDevNetwork);

        Self {
            network_params,
            nodes: Vec::new(),
            initialization_blocks: Vec::new(),
            initialization_blocks_cemented: Vec::new(),
        }
    }

    pub fn default_config() -> NodeConfig {
        let network_params = NetworkParams::new(NetworkType::NanoDevNetwork);
        let mut config = NodeConfig::new(Some(0), &network_params, 1);
        config.representative_vote_weight_minimum = Amount::ZERO;
        config.io_threads = 1;
        // process blocks sequentially
        config.block_processor.batch_size = 1;
        config.block_processor_threads = 1;
        config
    }

    pub fn default_config_without_backlog_scan() -> NodeConfig {
        NodeConfig {
            backlog_scan: BacklogScanConfig {
                enabled: false,
                ..Default::default()
            },
            ..Self::default_config()
        }
    }

    pub fn build_node(&mut self) -> TestNodeBuilder<'_> {
        TestNodeBuilder {
            system: self,
            config: None,
            flags: None,
            disconnected: false,
            event_sink: None,
        }
    }

    pub fn make_disconnected_node(&mut self) -> Arc<Node> {
        self.build_node().disconnected().finish()
    }

    pub fn make_node(&mut self) -> Arc<Node> {
        self.build_node().finish()
    }

    fn setup_node(&mut self, node: &Node) {
        for block in &self.initialization_blocks {
            node.ledger.process_one(block).unwrap();
        }

        for block in &self.initialization_blocks_cemented {
            node.ledger.process_one(block).unwrap();
            node.ledger.confirm(block.hash());
        }
    }

    fn make_node_with(
        &mut self,
        config: NodeConfig,
        flags: NodeFlags,
        disconnected: bool,
        event_sink: Option<SyncSender<NodeEvent>>,
    ) -> Arc<Node> {
        let mut node = self.new_node(config, flags, event_sink);

        self.setup_node(&node);

        let wallet_id = WalletId::random();
        node.wallets.create(wallet_id);
        node.start();

        // Check that we don't start more nodes than limit for single IP address
        debug_assert!(self.nodes.len() < node.config.network.max_peers_per_ip.into());
        let node = Arc::new(node);
        self.nodes.push(node.clone());

        if self.nodes.len() > 1 && !disconnected {
            let other = &self.nodes[0];
            let start = Instant::now();
            let node_addr = loop {
                let addr = node.tcp_listener.local_address();
                if addr.port() != 0 {
                    break addr;
                }
                if start.elapsed() > Duration::from_secs(5) {
                    panic!("node listener did not bind to a real port");
                }
                sleep(Duration::from_millis(10));
            };
            if let Err(e) = other.peer_connector.connect_to(node_addr) {
                panic!("Could not connect to {}. Reason: {:?}", node_addr, e);
            }

            let start = Instant::now();
            loop {
                if node
                    .network
                    .read()
                    .unwrap()
                    .find_node_id(&other.node_id.public_key().into())
                    .is_some()
                    && other
                        .network
                        .read()
                        .unwrap()
                        .find_node_id(&node.node_id.public_key().into())
                        .is_some()
                {
                    break;
                }

                if start.elapsed() > Duration::from_secs(7) {
                    panic!("connection not successfull");
                }
                sleep(Duration::from_millis(10));
            }
        }
        node
    }

    fn new_node(
        &self,
        config: NodeConfig,
        flags: NodeFlags,
        event_sink: Option<SyncSender<NodeEvent>>,
    ) -> Node {
        let mut flags = flags;
        flags.disable_ledger_storage = true;
        flags.disable_wallet_storage = true;
        let path = unique_path().expect("Could not get a unique path");
        let mut builder = NodeBuilder::new(self.network_params.network.current_network)
            .data_path(path)
            .config(config)
            .network_params(self.network_params.clone())
            .flags(flags);
        if let Some(sink) = event_sink {
            builder = builder.event_sink(sink);
        }
        builder.finish().unwrap()
    }

    fn stop(&mut self) {
        for node in self.nodes.drain(..) {
            Self::shutdown_node(node);
        }
    }

    pub fn stop_node(&mut self, node: Arc<Node>) {
        let index = self
            .nodes
            .iter()
            .position(|n| Arc::ptr_eq(n, &node))
            .unwrap();
        drop(node);
        let node = self.nodes.remove(index);
        Self::shutdown_node(node);
    }

    fn shutdown_node(mut node: Arc<Node>) {
        let data_path = node.data_path.clone();
        let start = Instant::now();
        loop {
            let n = Arc::get_mut(&mut node);
            if let Some(n) = n {
                n.stop();
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Could not get exclusive access to node!")
            }
            std::thread::yield_now();
        }
        drop(node);
        std::fs::remove_dir_all(&data_path).expect("Could not delete node data dir");
    }
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for System {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct TestNodeBuilder<'a> {
    system: &'a mut System,
    config: Option<NodeConfig>,
    flags: Option<NodeFlags>,
    disconnected: bool,
    event_sink: Option<SyncSender<NodeEvent>>,
}

impl TestNodeBuilder<'_> {
    pub fn config(mut self, cfg: NodeConfig) -> Self {
        self.config = Some(cfg);
        self
    }

    pub fn flags(mut self, flags: NodeFlags) -> Self {
        self.flags = Some(flags);
        self
    }

    pub fn disconnected(mut self) -> Self {
        self.disconnected = true;
        self
    }

    pub fn event_sink(mut self, sink: SyncSender<NodeEvent>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    pub fn finish(self) -> Arc<Node> {
        let config = self.config.unwrap_or_else(System::default_config);
        let flags = self.flags.unwrap_or_default();
        self.system
            .make_node_with(config, flags, self.disconnected, self.event_sink)
    }
}

pub fn assert_never(duration: Duration, mut check: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < duration {
        if check() {
            panic!("never check failed");
        }
        sleep(Duration::from_millis(50));
    }
}

pub fn assert_timely2<F>(check: F)
where
    F: FnMut() -> bool,
{
    assert_timely(Duration::from_secs(10), check);
}

pub fn assert_timely<F>(timeout: Duration, check: F)
where
    F: FnMut() -> bool,
{
    assert_timely_msg(timeout, check, "timeout");
}

pub fn assert_timely_msg<F>(timeout: Duration, mut check: F, error_message: &str)
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if check() {
            return;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("{}", error_message);
}

pub fn assert_timely_eq2<T, F>(check: F, expected: T)
where
    T: PartialEq + std::fmt::Debug + Clone,
    F: FnMut() -> T,
{
    assert_timely_eq(Duration::from_secs(10), check, expected)
}

pub fn assert_timely_eq<T, F>(timeout: Duration, mut check: F, expected: T)
where
    T: PartialEq + std::fmt::Debug + Clone,
    F: FnMut() -> T,
{
    let start = Instant::now();
    let mut actual = expected.clone();
    while start.elapsed() < timeout {
        actual = check();
        if actual == expected {
            return;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("timeout. expected: {expected:?}, actual: {actual:?}");
}

pub fn assert_always_eq<T, F>(time: Duration, mut condition: F, expected: T)
where
    T: PartialEq + std::fmt::Debug,
    F: FnMut() -> T,
{
    let start = Instant::now();
    while start.elapsed() < time {
        assert_eq!(condition(), expected);
        sleep(Duration::from_millis(50));
    }
}

static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();

pub fn init_tracing() {
    TRACING_INITIALIZED.get_or_init(|| {
        TracingInitializer::default().init_with_default_log_level("off");
    });
}

pub fn establish_tcp(node: &Node, peer: &Node) -> Arc<Channel> {
    node.peer_connector
        .connect_to(peer.tcp_listener.local_address())
        .unwrap();

    assert_timely_msg(
        Duration::from_secs(2),
        || {
            node.network
                .read()
                .unwrap()
                .find_node_id(&peer.node_id.public_key().into())
                .is_some()
        },
        "node did not connect",
    );

    node.network
        .read()
        .unwrap()
        .find_node_id(&peer.node_id.public_key().into())
        .unwrap()
        .clone()
}

pub fn make_fake_channel(node: &Node) -> Arc<Channel> {
    node.network
        .write()
        .unwrap()
        .add(
            NULL_ENDPOINT,
            TEST_ENDPOINT_1,
            ChannelDirection::Inbound,
            node.steady_clock.now(),
        )
        .unwrap()
        .0
}

pub fn start_election(node: &Node, hash: &BlockHash) {
    assert_timely2(|| node.block_exists(hash));

    let block = node.block(hash).unwrap();
    node.election_schedulers.add_manual(block.clone());
    // wait for the election to appear
    assert_timely2(|| node.is_active_root(&block.qualified_root()));
    node.active.transition_active(&block.hash());
}

pub fn start_elections(node: &Node, hashes: &[BlockHash], forced: bool) {
    for hash in hashes {
        start_election(node, hash);
        if forced {
            node.force_confirm(hash);
        }
    }
}

pub fn activate_hashes(node: &Node, hashes: &[BlockHash]) {
    for hash in hashes {
        let block = node.block(hash).unwrap();
        node.election_schedulers.add_manual(block);
    }
}

pub fn setup_chain(
    node: &Node,
    count: usize,
    target: &PrivateKey,
    confirm: bool,
) -> Vec<SavedBlock> {
    let mut latest = node.latest(&target.account());
    let mut balance = node.balance(&target.account());

    let mut blocks = Vec::with_capacity(count);
    let mut result = Vec::with_capacity(count);

    for _ in 0..count {
        let throwaway = PrivateKey::new();
        balance -= Amount::raw(1);
        let send: Block = StateBlockArgs {
            key: target,
            previous: latest,
            representative: target.public_key(),
            balance,
            link: throwaway.account().into(),
            work: node.work_generate_dev(latest),
        }
        .into();
        latest = send.hash();
        blocks.push(send);
    }

    for block in &blocks {
        let saved = node.process(block.clone());
        result.push(saved);
    }

    if confirm {
        // Confirm whole chain at once
        for block in &blocks {
            node.confirm(block.hash());
        }
    }

    assert_timely2(|| {
        let empty = node.active.is_empty();
        if !empty {
            node.active.cancel_all();
        }
        empty
    });

    result
}

pub fn setup_chains(
    node: &Node,
    chain_count: usize,
    block_count: usize,
    source: &PrivateKey,
    confirm: bool,
) -> Vec<(Account, Vec<SavedBlock>)> {
    let mut latest = node.latest(&source.account());
    let mut balance = node.balance(&source.account());

    let mut chains = Vec::new();
    for _ in 0..chain_count {
        let key = PrivateKey::new();
        let amount_sent = Amount::raw(block_count as u128 * 2);
        balance -= amount_sent; // Send enough to later create `block_count` blocks
        let send: Block = StateBlockArgs {
            key: source,
            previous: latest,
            representative: source.public_key(),
            balance,
            link: key.account().into(),
            work: node.work_generate_dev(latest),
        }
        .into();

        let open: Block = StateBlockArgs {
            key: &key,
            previous: BlockHash::ZERO,
            representative: key.public_key(),
            balance: amount_sent,
            link: send.hash().into(),
            work: node.work_generate_dev(&key),
        }
        .into();

        latest = send.hash();
        node.process(send.clone());
        let open = node.process(open);

        if confirm {
            node.confirm(send.hash());
            node.confirm(open.hash());
        }

        let mut blocks = setup_chain(node, block_count, &key, confirm);
        blocks.insert(0, open);

        chains.push((key.account(), blocks));

        assert_timely2(|| {
            let empty = node.active.is_empty();
            if !empty {
                node.active.cancel_all();
            }
            empty
        });
    }

    chains
}

pub fn setup_independent_blocks(node: &Node, count: usize, source: &PrivateKey) -> Vec<SavedBlock> {
    let mut blocks = Vec::new();
    let account: Account = source.public_key().into();
    let mut latest = node.latest(&account);
    let mut balance = node.balance(&account);

    for _ in 0..count {
        let key = PrivateKey::new();

        balance -= 1;

        let send: Block = StateBlockArgs {
            key: source,
            previous: latest,
            representative: source.public_key(),
            balance,
            link: key.account().into(),
            work: node.work_generate_dev(latest),
        }
        .into();

        latest = send.hash();

        let open: Block = StateBlockArgs {
            key: &key,
            previous: BlockHash::ZERO,
            representative: key.public_key(),
            balance: Amount::raw(1),
            link: send.hash().into(),
            work: node.work_generate_dev(&key),
        }
        .into();

        node.process(send.clone());
        let open = node.process(open);
        // Ensure blocks are in the ledger
        assert_timely(Duration::from_secs(5), || {
            node.block_hashes_exist([send.hash(), open.hash()])
        });

        blocks.push(open);
    }

    // Confirm whole genesis chain at once
    node.confirm(latest);

    blocks
}

use rsnano_nullable_tcp::get_available_port;
use rsnano_nullable_tracing_subscriber::TracingInitializer;
use tokio::net::TcpListener as TokioTcpListener;

pub struct RpcServerGuard {
    handle: tokio::runtime::Handle,
    pub client: Arc<NanoRpcClient>,
    tx_stop: Option<tokio::sync::oneshot::Sender<()>>,
    rx_closed: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl Drop for RpcServerGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx_stop.take() {
            let _ = tx.send(());
            let _ = self.handle.block_on(self.rx_closed.take().unwrap());
        }
    }
}

pub fn setup_rpc_client_and_server(node: Arc<Node>, enable_control: bool) -> RpcServerGuard {
    let port = get_available_port();
    let socket_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    let rpc_url = format!("http://[::1]:{}/", port);
    let rpc_client = Arc::new(NanoRpcClient::new(Url::parse(&rpc_url).unwrap()));

    let listener = node.runtime.block_on(async {
        TokioTcpListener::bind(socket_addr)
            .await
            .expect("Failed to bind to address")
    });

    let (tx_stop, rx_stop) = tokio::sync::oneshot::channel();
    let (tx_stop2, rx_stop2) = tokio::sync::oneshot::channel();
    let (tx_closed, rx_closed) = tokio::sync::oneshot::channel();

    let node_l = node.clone();
    node.runtime.spawn(async move {
        let result = run_rpc_server(node_l, listener, enable_control, tx_stop, async move {
            tokio::select! {
                _ = rx_stop => {},
                _ = rx_stop2 => {}
            }
        })
        .await;

        let _ = tx_closed.send(());

        result.unwrap();
    });

    RpcServerGuard {
        handle: node.runtime.clone(),
        client: rpc_client,
        tx_stop: Some(tx_stop2),
        rx_closed: Some(rx_closed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_node_removes_data_path() {
        let mut system = System::new();
        let node = system.make_node();
        let data_path = node.data_path.clone();

        assert!(data_path.exists());

        system.stop_node(node);

        assert!(!data_path.exists());
    }

    #[test]
    fn dropping_system_removes_node_data_paths() {
        let data_path = {
            let mut system = System::new();
            let node = system.make_node();
            let data_path = node.data_path.clone();

            assert!(data_path.exists());

            data_path
        };

        assert!(!data_path.exists());
    }
}

pub fn send_block(node: Arc<Node>) -> BlockHash {
    let send1 = send_block_to(node, *DEV_GENESIS_ACCOUNT, Amount::raw(1));
    send1.hash()
}

pub fn send_block_to(node: Arc<Node>, account: Account, amount: Amount) -> Block {
    let any = node.ledger.any();

    let previous = any
        .account_head(&DEV_GENESIS_ACCOUNT)
        .unwrap_or(*DEV_GENESIS_HASH);

    let balance = any.account_balance(&DEV_GENESIS_ACCOUNT);

    let send: Block = StateBlockArgs {
        key: &DEV_GENESIS_KEY,
        previous,
        representative: *DEV_GENESIS_PUB_KEY,
        balance: balance - amount,
        link: account.into(),
        work: node.work_generate_dev(previous),
    }
    .into();

    node.process_active(send.clone());
    assert_timely2(|| node.is_active_root(&send.qualified_root()));

    send
}

pub fn process_send_block(node: Arc<Node>, account: Account, amount: Amount) -> Block {
    let any = node.ledger.any();

    let previous = any
        .account_head(&DEV_GENESIS_ACCOUNT)
        .unwrap_or(*DEV_GENESIS_HASH);

    let balance = any.account_balance(&DEV_GENESIS_ACCOUNT);

    let send: Block = StateBlockArgs {
        key: &DEV_GENESIS_KEY,
        previous,
        representative: *DEV_GENESIS_PUB_KEY,
        balance: balance - amount,
        link: account.into(),
        work: node.work_generate_dev(previous),
    }
    .into();

    node.process(send.clone());

    send
}

pub fn process_open_block(node: Arc<Node>, keys: PrivateKey) -> Block {
    let any = node.ledger.any();
    let account = keys.account();

    let (key, info) = any
        .account_receivable_upper_bound(account, BlockHash::ZERO)
        .next()
        .unwrap();

    let open: Block = StateBlockArgs {
        key: &keys,
        previous: BlockHash::ZERO,
        representative: keys.public_key(),
        balance: info.amount,
        link: key.send_block_hash.into(),
        work: node.work_generate_dev(account),
    }
    .into();

    node.process(open.clone());

    open
}

pub fn upgrade_epoch(node: Arc<Node>, epoch: Epoch) -> Block {
    let any = node.ledger.any();
    let account = *DEV_GENESIS_ACCOUNT;
    let latest = any.account_head(&account).unwrap();
    let balance = any.account_balance(&account);

    let epoch_block: Block = StateBlockArgs {
        key: &DEV_GENESIS_KEY,
        previous: latest,
        representative: *DEV_GENESIS_PUB_KEY,
        balance,
        link: node.ledger.epoch_link(epoch).unwrap(),
        work: node.work_generate_dev(*DEV_GENESIS_HASH),
    }
    .into();

    node.process(epoch_block.clone());

    epoch_block
}

pub fn setup_rep(node: &Node, amount: Amount, source: &PrivateKey) -> PrivateKey {
    let destkey = PrivateKey::new();
    setup_new_account(node, amount, source, &destkey, destkey.public_key(), true);
    destkey
}

pub fn setup_new_account(
    node: &Node,
    amount: Amount,
    source: &PrivateKey,
    dest: &PrivateKey,
    dest_rep: PublicKey,
    force_confirm: bool,
) -> (Block, Block) {
    let source_account = source.public_key().as_account();
    let dest_account = dest.public_key().as_account();
    let latest = node.latest(&source_account);
    let balance = node.balance(&source_account);

    let send: Block = StateBlockArgs {
        key: source,
        previous: latest,
        representative: source.public_key(),
        balance: balance - amount,
        link: dest_account.into(),
        work: node.work_generate_dev(latest),
    }
    .into();

    let open: Block = StateBlockArgs {
        key: dest,
        previous: BlockHash::ZERO,
        representative: dest_rep,
        balance: amount,
        link: send.hash().into(),
        work: node.work_generate_dev(dest_account),
    }
    .into();

    node.process(send.clone());
    node.process(open.clone());

    if force_confirm {
        node.confirm(send.hash());
        node.confirm(open.hash());
    }

    (send, open)
}
