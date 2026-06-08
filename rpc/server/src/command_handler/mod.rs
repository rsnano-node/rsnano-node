mod ledger;
mod node;
mod utils;
mod wallets;

use anyhow::anyhow;
use rsnano_ledger::AnySet;
use rsnano_node::Node;
use rsnano_rpc_messages::{RpcCommand, RpcError, StatsType};
use rsnano_types::{Account, AccountInfo, BlockHash, SavedBlock};
use serde_json::{Value, to_value};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tracing::debug;
use utils::*;

#[derive(Clone)]
pub(crate) struct RpcCommandHandler {
    node: Arc<Node>,
    enable_control: bool,
    stop: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl RpcCommandHandler {
    pub fn new(node: Arc<Node>, enable_control: bool, tx_stop: oneshot::Sender<()>) -> Self {
        Self {
            node,
            enable_control,
            stop: Arc::new(Mutex::new(Some(tx_stop))),
        }
    }

    pub fn handle(&self, command: RpcCommand) -> serde_json::Value {
        debug!(?command, "Handling RPC command");
        self.call_handler(command).unwrap_or_else(Self::error_value)
    }

    fn error_value(error: anyhow::Error) -> serde_json::Value {
        serde_json::to_value(RpcError::new(error.to_string())).unwrap()
    }

    fn call_handler(&self, command: RpcCommand) -> anyhow::Result<serde_json::Value> {
        self.check_control_enabled(&command)?;
        let response = match command {
            RpcCommand::AccountBalance(args) => to_value(self.account_balance(args)),
            RpcCommand::AccountBlockCount(args) => to_value(self.account_block_count(args)?),
            RpcCommand::AccountCreate(args) => to_value(self.account_create(args)?),
            RpcCommand::AccountGet(args) => to_value(account_get(args)),
            RpcCommand::AccountHistory(args) => to_value(self.account_history(args)?),
            RpcCommand::AccountInfo(args) => to_value(self.account_info(args)?),
            RpcCommand::AccountKey(args) => to_value(account_key(args)),
            RpcCommand::AccountList(args) => to_value(self.account_list(args)?),
            RpcCommand::AccountMove(args) => to_value(self.account_move(args)?),
            RpcCommand::AccountsReceivable(args) => to_value(self.accounts_receivable(args)),
            RpcCommand::AccountRemove(args) => to_value(self.account_remove(args)?),
            RpcCommand::AccountRepresentative(args) => to_value(self.account_representative(args)?),
            RpcCommand::AccountWeight(args) => to_value(self.account_weight(args)),
            RpcCommand::AccountsRepresentatives(args) => {
                to_value(self.accounts_representatives(args))
            }
            RpcCommand::AccountsCreate(args) => to_value(self.accounts_create(args)?),
            RpcCommand::AccountsFrontiers(args) => to_value(self.accounts_frontiers(args)),
            RpcCommand::AvailableSupply => to_value(self.available_supply()),
            RpcCommand::BlockInfo(args) => to_value(self.block_info(args)?),
            RpcCommand::BlocksInfo(args) => to_value(self.blocks_info(args)?),
            RpcCommand::Blocks(args) => to_value(self.blocks(args)?),
            RpcCommand::BlockConfirm(args) => to_value(self.block_confirm(args)?),
            RpcCommand::BlockAccount(args) => to_value(self.block_account(args)?),
            RpcCommand::BlockCount => to_value(self.block_count()),
            RpcCommand::Receive(args) => to_value(self.receive(args)?),
            RpcCommand::BlockCreate(args) => to_value(self.block_create(args)?),
            RpcCommand::BlockHash(args) => to_value(block_hash(args)),
            RpcCommand::Bootstrap(_)
            | RpcCommand::BootstrapAny(_)
            | RpcCommand::BootstrapLazy(_) => to_value(legacy_bootstrap_disabled()?),
            RpcCommand::ConfirmationActive(args) => to_value(self.confirmation_active(args)),
            RpcCommand::ConfirmationInfo(args) => to_value(self.confirmation_info(args)?),
            RpcCommand::ConfirmationQuorum(args) => to_value(self.confirmation_quorum(args)),
            RpcCommand::Delegators(args) => to_value(self.delegators(args)),
            RpcCommand::DelegatorsCount(args) => to_value(self.delegators_count(args)),
            RpcCommand::DeterministicKey(args) => to_value(deterministic_key(args)),
            RpcCommand::Frontiers(args) => to_value(self.frontiers(args)),
            RpcCommand::FrontierCount => to_value(self.frontier_count()),
            RpcCommand::Keepalive(args) => to_value(self.keepalive(args)?),
            RpcCommand::KeyCreate => to_value(key_create()),
            RpcCommand::KeyExpand(args) => to_value(key_expand(args)?),
            RpcCommand::NodeId => to_value(self.node_id()),
            RpcCommand::PasswordChange(args) => to_value(self.password_change(args)?),
            RpcCommand::PasswordEnter(args) => to_value(self.password_enter(args)?),
            RpcCommand::Peers(args) => to_value(self.peers(args)),
            RpcCommand::ReceivableExists(args) => to_value(self.receivable_exists(args)?),
            RpcCommand::ReceiveMinimum => to_value(self.receive_minimum()),
            RpcCommand::RepresentativesOnline(args) => to_value(self.representatives_online(args)),
            RpcCommand::SearchReceivable(args) => to_value(self.search_receivable(args)?),
            RpcCommand::SearchReceivableAll => to_value(self.search_receivable_all()),
            RpcCommand::UncheckedClear => to_value(self.unchecked_clear()),
            RpcCommand::Unchecked(args) => to_value(self.unchecked(args)),
            RpcCommand::UncheckedGet(args) => to_value(self.unchecked_get(args)?),
            RpcCommand::WalletAdd(args) => to_value(self.wallet_add(args)?),
            RpcCommand::WalletAddWatch(args) => to_value(self.wallet_add_watch(args)?),
            RpcCommand::WalletBalances(args) => to_value(self.wallet_balances(args)?),
            RpcCommand::PopulateBacklog => to_value(self.populate_backlog()),
            RpcCommand::ValidateAccountNumber(args) => to_value(validate_account_number(args)),
            RpcCommand::UncheckedKeys(args) => to_value(self.unchecked_keys(args)),
            RpcCommand::WalletChangeSeed(args) => to_value(self.wallet_change_seed(args)),
            RpcCommand::WalletContains(args) => to_value(self.wallet_contains(args)?),
            RpcCommand::WalletCreate(args) => to_value(self.wallet_create(args)?),
            RpcCommand::WalletDestroy(args) => to_value(self.wallet_destroy(args)?),
            RpcCommand::WalletExport(args) => to_value(self.wallet_export(args)?),
            RpcCommand::WalletFrontiers(args) => to_value(self.wallet_frontiers(args)?),
            RpcCommand::WalletInfo(args) => to_value(self.wallet_info(args)?),
            RpcCommand::PasswordValid(args) => to_value(self.password_valid(args)?),
            RpcCommand::WalletLocked(args) => to_value(self.wallet_locked(args)?),
            RpcCommand::WalletLedger(args) => to_value(self.wallet_ledger(args)?),
            RpcCommand::WalletLock(args) => to_value(self.wallet_lock(args)?),
            RpcCommand::WalletRepresentative(args) => to_value(self.wallet_representative(args)?),
            RpcCommand::WalletRepresentativeSet(args) => {
                to_value(self.wallet_representative_set(args)?)
            }
            RpcCommand::WalletRepublish(args) => to_value(self.wallet_republish(args)?),
            RpcCommand::WalletWorkGet(args) => to_value(self.wallet_work_get(args)?),
            RpcCommand::WorkCancel(args) => to_value(self.work_cancel(args)),
            RpcCommand::WorkGet(args) => to_value(self.work_get(args)?),
            RpcCommand::WorkSet(args) => to_value(self.work_set(args)?),
            RpcCommand::WorkValidate(args) => to_value(self.work_validate(args)),
            RpcCommand::Uptime => to_value(self.uptime()),
            RpcCommand::NanoToRaw(args) => to_value(nano_to_raw(args)?),
            RpcCommand::RawToNano(args) => to_value(raw_to_nano(args)),
            RpcCommand::Ledger(args) => to_value(self.ledger(args)),
            RpcCommand::Receivable(args) => to_value(self.receivable(args)),
            RpcCommand::Stop => to_value(self.stop()),
            RpcCommand::Representatives(args) => to_value(self.representatives(args)),
            RpcCommand::StatsClear => to_value(self.stats_clear()),
            RpcCommand::Unopened(args) => to_value(self.unopened(args)),
            RpcCommand::Send(args) => to_value(self.send(args)?),
            RpcCommand::AccountsBalances(args) => to_value(self.accounts_balances(args)),
            RpcCommand::Chain(args) => to_value(self.chain(args, false)),
            RpcCommand::Successors(args) => to_value(self.chain(args, true)),
            RpcCommand::Sign(args) => to_value(self.sign(args)?),
            RpcCommand::Process(args) => to_value(self.process(args)?),
            RpcCommand::Republish(args) => to_value(self.republish(args)?),
            RpcCommand::WalletHistory(args) => to_value(self.wallet_history(args)?),
            RpcCommand::Telemetry(args) => to_value(self.telemetry(args)?),
            RpcCommand::WorkGenerate(args) => to_value(self.work_generate(args)?),
            RpcCommand::WalletReceivable(args) => to_value(self.wallet_receivable(args)?),
            RpcCommand::Stats(args) => Ok(self.stats(args)?),
            RpcCommand::ConfirmationHistory(args) => to_value(self.confirmation_history(args)),
            RpcCommand::Version => to_value(self.version()),
            RpcCommand::ActiveDifficulty => to_value(self.active_difficulty()),
            #[cfg(feature = "ledger_snapshots")]
            RpcCommand::StartLedgerSnapshot => to_value(self.start_ledger_snapshot()),

            // Not implemented:
            RpcCommand::AccountRepresentativeSet(_) => self.not_implemented(),
            RpcCommand::WorkPeers => to_value(self.work_peers()),
            RpcCommand::WorkPeerAdd(args) => to_value(self.work_peer_add(args)),
            RpcCommand::WorkPeersClear => to_value(self.work_peers_clear()),
            RpcCommand::DatabaseTxnTracker(_) => self.not_implemented(),
            RpcCommand::ReceiveMinimumSet(_) => self.not_implemented(),
        }?;

        Ok(response)
    }

    fn check_control_enabled(&self, command: &RpcCommand) -> anyhow::Result<()> {
        if !self.enable_control && requires_control(command) {
            Err(anyhow!("RPC control is disabled"))
        } else {
            Ok(())
        }
    }

    fn load_block_any(&self, any: &dyn AnySet, hash: &BlockHash) -> anyhow::Result<SavedBlock> {
        any.get_block(hash)
            .ok_or_else(|| anyhow!(Self::BLOCK_NOT_FOUND))
    }

    fn load_account(&self, any: &dyn AnySet, account: &Account) -> anyhow::Result<AccountInfo> {
        any.get_account(account)
            .ok_or_else(|| anyhow!(Self::ACCOUNT_NOT_FOUND))
    }

    const BLOCK_NOT_FOUND: &str = "Block not found";
    const NOT_IMPLEMENTED: &str = "Not implemented yet";
    const ACCOUNT_NOT_FOUND: &str = "Account not found";

    fn not_implemented(&self) -> Result<Value, serde_json::Error> {
        Ok(Value::String(Self::NOT_IMPLEMENTED.to_string()))
    }
}

fn requires_control(command: &RpcCommand) -> bool {
    match command {
        RpcCommand::AccountCreate(_)
        | RpcCommand::AccountMove(_)
        | RpcCommand::AccountRemove(_)
        | RpcCommand::AccountRepresentativeSet(_)
        | RpcCommand::AccountsCreate(_)
        | RpcCommand::BlockCreate(_)
        | RpcCommand::BootstrapLazy(_)
        | RpcCommand::DatabaseTxnTracker(_)
        | RpcCommand::Keepalive(_)
        | RpcCommand::Ledger(_)
        | RpcCommand::NodeId
        | RpcCommand::PasswordChange(_)
        | RpcCommand::PopulateBacklog
        | RpcCommand::Receive(_)
        | RpcCommand::ReceiveMinimum
        | RpcCommand::ReceiveMinimumSet(_)
        | RpcCommand::SearchReceivable(_)
        | RpcCommand::SearchReceivableAll
        | RpcCommand::Send(_)
        | RpcCommand::Sign(_)
        | RpcCommand::Stop
        | RpcCommand::UncheckedClear
        | RpcCommand::Unopened(_)
        | RpcCommand::WalletAdd(_)
        | RpcCommand::WalletAddWatch(_)
        | RpcCommand::WalletChangeSeed(_)
        | RpcCommand::WalletCreate(_)
        | RpcCommand::WalletDestroy(_)
        | RpcCommand::WalletLock(_)
        | RpcCommand::WalletLedger(_)
        | RpcCommand::WalletRepresentativeSet(_)
        | RpcCommand::WalletReceivable(_)
        | RpcCommand::WalletRepublish(_)
        | RpcCommand::WalletExport(_)
        | RpcCommand::WalletWorkGet(_)
        | RpcCommand::WorkGenerate(_)
        | RpcCommand::WorkCancel(_)
        | RpcCommand::WorkGet(_)
        | RpcCommand::WorkSet(_)
        | RpcCommand::WorkPeerAdd(_)
        | RpcCommand::WorkPeers
        | RpcCommand::WorkPeersClear => true,
        RpcCommand::Stats(s) => matches!(s.stats_type, StatsType::Objects),
        RpcCommand::Process(args) => args.force == Some(true.into()),
        _ => false,
    }
}

fn legacy_bootstrap_disabled() -> anyhow::Result<serde_json::Value> {
    Err(anyhow!("Legacy bootstrap is disabled"))
}

#[cfg(test)]
use serde::de::DeserializeOwned;

#[cfg(test)]
pub fn test_rpc_command_requires_control(cmd: RpcCommand) {
    let node = Arc::new(Node::new_null());
    let (tx_stop, _rx_stop) = tokio::sync::oneshot::channel();
    let cmd_handler = RpcCommandHandler::new(node, false, tx_stop);
    let result = cmd_handler.handle(cmd);
    let error: RpcError = serde_json::from_value(result).unwrap();
    assert_eq!(error.error, "RPC control is disabled");
}

#[cfg(test)]
pub fn test_rpc_command<T>(cmd: RpcCommand) -> T
where
    T: DeserializeOwned,
{
    let node = Arc::new(Node::new_null());
    test_rpc_command_with_node(cmd, node)
}

#[cfg(test)]
pub fn test_rpc_command_with_node<T>(cmd: RpcCommand, node: Arc<Node>) -> T
where
    T: DeserializeOwned,
{
    let (tx_stop, _rx_stop) = tokio::sync::oneshot::channel();
    let cmd_handler = RpcCommandHandler::new(node, true, tx_stop);
    let result = cmd_handler.handle(cmd);
    serde_json::from_value(result).unwrap()
}
