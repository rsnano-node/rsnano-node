use anyhow::{anyhow, Result};
use reqwest::Url;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    time::Duration,
};
use tokio::time::sleep;

use rsnano::{
    config::{
        get_node_toml_config_path, get_rpc_toml_config_path, DaemonConfig, NetworkConstants,
        RpcConfig,
    },
    ledger::DEV_GENESIS_KEY,
    unique_path,
    utils::TomlConfig,
    NetworkParams, DEV_NETWORK_PARAMS,
};

use crate::create_send_and_receive_blocks;
use crate::Account;
use crate::AccountInfo;
use crate::RpcClient;

const RPC_PORT_START: u16 = 60000;
const PEERING_PORT_START: u16 = 61000;
const IPC_PORT_START: u16 = 62000;

pub struct TestNode {
    node_no: usize,
    pub data_path: PathBuf,
    node_child: Option<Child>,
    rpc_child: Option<Child>,
    node_client: Arc<RpcClient>,
    pub rpc_port: u16,
    pub peering_port: u16,
}

impl TestNode {
    pub fn new(node_no: usize) -> Result<Self> {
        let data_path = unique_path().ok_or_else(|| anyhow!("no unique path"))?;
        let rpc_port = RPC_PORT_START + node_no as u16;
        let peering_port = PEERING_PORT_START + node_no as u16;
        let node_url = format!("http://[::1]:{}/", rpc_port);
        let node_client = Arc::new(RpcClient::new(Url::parse(&node_url)?));
        Ok(Self {
            node_no,
            data_path,
            node_child: None,
            rpc_child: None,
            node_client,
            rpc_port,
            peering_port,
        })
    }

    pub async fn start(&mut self, node_path: &Path, rpc_path: &Path) -> Result<()> {
        std::fs::create_dir(self.data_path.as_path())?;
        write_config_files(self.data_path.as_path(), self.node_no)?;
        let current_network = DEV_NETWORK_PARAMS.network.get_current_network_as_string();
        self.node_child = Some(spawn_nano_node(node_path, &self.data_path, current_network));
        self.rpc_child = Some(spawn_nano_rpc(rpc_path, &self.data_path, current_network));
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.node_client.stop_rpc().await?;

        if let Some(c) = self.node_child.take() {
            wait_for_child_to_exit(c).await;
        }

        if let Some(c) = self.rpc_child.take() {
            wait_for_child_to_exit(c).await;
        }
        Ok(())
    }

    pub async fn connect(&self, other: &TestNode) -> Result<()> {
        self.node_client.keepalive_rpc(other.peering_port).await
    }

    pub async fn create_send_and_receive_blocks(
        &self,
        destination_count: usize,
        send_count: usize,
        simultaneous_process_calls: usize,
    ) -> Result<HashMap<String, AccountInfo>> {
        let destination_accounts = self.create_destination_accounts(destination_count).await?;
        let wallet = self.node_client.wallet_create_rpc().await?;
        self.add_genesis_account(&wallet).await?;
        self.add_destination_accounts(&destination_accounts, &wallet)
            .await?;

        create_send_and_receive_blocks(
            send_count,
            simultaneous_process_calls,
            destination_accounts,
            wallet,
            self.node_client.clone(),
        )
        .await
    }

    async fn add_genesis_account(&self, wallet: &str) -> Result<()> {
        self.node_client
            .wallet_add_rpc(wallet, &DEV_GENESIS_KEY.private_key().encode_hex())
            .await
    }

    async fn add_destination_accounts(
        &self,
        destination_accounts: &[Account],
        wallet: &str,
    ) -> Result<()> {
        for account in destination_accounts {
            self.node_client
                .wallet_add_rpc(wallet, &account.private_key)
                .await?;
        }
        Ok(())
    }

    async fn create_destination_accounts(&self, destination_count: usize) -> Result<Vec<Account>> {
        let mut destination_accounts = Vec::with_capacity(destination_count);
        for _ in 0..destination_count {
            let acc = self.node_client.key_create_rpc().await?;
            destination_accounts.push(acc);
        }
        Ok(destination_accounts)
    }

    pub async fn account_info(&self, account: &str) -> Result<AccountInfo> {
        self.node_client.account_info_rpc(account).await
    }
}

async fn wait_for_child_to_exit(mut child: Child) {
    loop {
        if child.try_wait().is_ok() {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

fn spawn_nano_rpc(rpc_path: &Path, data_path: &Path, network: &str) -> Child {
    Command::new(rpc_path.as_os_str())
        .arg("--daemon")
        .arg("--data_path")
        .arg(data_path)
        .arg("--network")
        .arg(network)
        .spawn()
        .expect("could not spawn rpc server")
}

fn spawn_nano_node(node_path: &Path, data_path: &Path, network: &str) -> Child {
    Command::new(node_path.as_os_str())
        .arg("--daemon")
        .arg("--data_path")
        .arg(data_path)
        .arg("--network")
        .arg(network)
        .spawn()
        .expect("could not spawn node")
}

fn write_config_files(data_path: &Path, index: usize) -> Result<()> {
    let network_params = NetworkParams::new(NetworkConstants::active_network())?;
    write_node_config(index, data_path, &network_params)?;
    write_rpc_config(index, data_path, &network_params)?;
    Ok(())
}

fn write_node_config(index: usize, data_path: &Path, network_params: &NetworkParams) -> Result<()> {
    let mut daemon_config = DaemonConfig::new(network_params)?;
    daemon_config.node.peering_port = Some(PEERING_PORT_START + index as u16);
    daemon_config
        .node
        .ipc_config
        .transport_tcp
        .transport
        .enabled = true;
    daemon_config.node.ipc_config.transport_tcp.port = IPC_PORT_START + index as u16;
    daemon_config.node.use_memory_pools = (index % 2) == 0;
    let mut toml = TomlConfig::new();
    daemon_config.serialize_toml(&mut toml)?;
    toml.write(get_node_toml_config_path(data_path))?;
    Ok(())
}

fn write_rpc_config(index: usize, data_path: &Path, network_params: &NetworkParams) -> Result<()> {
    let mut rpc_config = RpcConfig::new(&network_params.network);
    rpc_config.port = RPC_PORT_START + index as u16;
    rpc_config.enable_control = true;
    rpc_config.rpc_process.ipc_port = IPC_PORT_START + index as u16;
    let mut toml_rpc = TomlConfig::new();
    rpc_config.serialize_toml(&mut toml_rpc)?;
    toml_rpc.write(get_rpc_toml_config_path(data_path))?;
    Ok(())
}
