use super::{
    account_balance, account_block_count, account_create, account_get, account_history,
    account_info, account_key, account_list, account_move, account_remove, account_representative,
    account_weight, accounts_balances, accounts_create, accounts_frontiers, accounts_receivable,
    accounts_representatives, available_supply, block_account, block_confirm, block_count,
    block_create, block_hash, block_info, blocks, blocks_info, bootstrap, bootstrap_any,
    bootstrap_lazy, chain, confirmation_active, confirmation_info, confirmation_quorum, delegators,
    delegators_count, deterministic_key, frontier_count, frontiers, keepalive, key_create,
    key_expand, ledger, nano_to_raw, node_id, password_change, password_enter, password_valid,
    peers, populate_backlog, process, raw_to_nano, receivable, receivable_exists, receive_minimum,
    representatives, representatives_online, republish, search_receivable, search_receivable_all,
    send, sign, stats_clear, stop, unchecked, unchecked_clear, unchecked_get, unchecked_keys,
    unopened, uptime, validate_account_number, wallet_add, wallet_add_watch, wallet_balances,
    wallet_change_seed, wallet_contains, wallet_create, wallet_destroy, wallet_export,
    wallet_frontiers, wallet_history, wallet_info, wallet_ledger, wallet_lock, wallet_locked,
    wallet_receivable, wallet_representative, wallet_representative_set, wallet_republish,
    wallet_work_get, work_cancel, work_generate, work_get, work_set, work_validate,
};
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::map_request,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use rsnano_node::Node;
use rsnano_rpc_messages::RpcCommand;
use serde_json::to_string_pretty;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
struct RpcService {
    node: Arc<Node>,
    enable_control: bool,
}

pub async fn run_rpc_server(
    node: Arc<Node>,
    listener: TcpListener,
    enable_control: bool,
) -> Result<()> {
    let rpc_service = RpcService {
        node,
        enable_control,
    };

    let app = Router::new()
        .route("/", post(handle_rpc))
        .layer(map_request(set_header))
        .with_state(rpc_service);

    info!("RPC listening address: {}", listener.local_addr()?);

    axum::serve(listener, app)
        .await
        .context("Failed to run the server")?;

    Ok(())
}

async fn handle_rpc(
    State(rpc_service): State<RpcService>,
    Json(rpc_command): Json<RpcCommand>,
) -> Response {
    let response = match rpc_command {
        RpcCommand::AccountCreate(args) => {
            account_create(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountBalance(args) => account_balance(rpc_service.node, args).await,
        RpcCommand::AccountsCreate(args) => {
            accounts_create(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountRemove(args) => {
            account_remove(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountMove(args) => {
            account_move(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountList(args) => account_list(rpc_service.node, args).await,
        RpcCommand::WalletCreate(args) => {
            wallet_create(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::KeyCreate => key_create().await,
        RpcCommand::WalletAdd(args) => {
            wallet_add(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletContains(args) => wallet_contains(rpc_service.node, args).await,
        RpcCommand::WalletDestroy(args) => {
            wallet_destroy(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletLock(args) => {
            wallet_lock(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletLocked(args) => wallet_locked(rpc_service.node, args).await,
        RpcCommand::Stop => stop(rpc_service.node, rpc_service.enable_control).await,
        RpcCommand::AccountBlockCount(args) => account_block_count(rpc_service.node, args).await,
        RpcCommand::AccountKey(args) => account_key(args).await,
        RpcCommand::AccountGet(args) => account_get(args).await,
        RpcCommand::AccountRepresentative(args) => {
            account_representative(rpc_service.node, args).await
        }
        RpcCommand::AccountWeight(args) => account_weight(rpc_service.node, args).await,
        RpcCommand::AvailableSupply => available_supply(rpc_service.node).await,
        RpcCommand::BlockConfirm(args) => block_confirm(rpc_service.node, args).await,
        RpcCommand::BlockCount => block_count(rpc_service.node).await,
        RpcCommand::BlockAccount(args) => block_account(rpc_service.node, args).await,
        RpcCommand::Uptime => uptime(rpc_service.node).await,
        RpcCommand::Keepalive(args) => {
            keepalive(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::FrontierCount => frontier_count(rpc_service.node).await,
        RpcCommand::ValidateAccountNumber(_) => validate_account_number().await,
        RpcCommand::NanoToRaw(args) => nano_to_raw(args).await,
        RpcCommand::RawToNano(args) => raw_to_nano(args).await,
        RpcCommand::WalletAddWatch(args) => {
            wallet_add_watch(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletRepresentative(args) => {
            wallet_representative(rpc_service.node, args).await
        }
        RpcCommand::WorkSet(args) => {
            work_set(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WorkGet(args) => {
            work_get(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletWorkGet(args) => {
            wallet_work_get(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountsFrontiers(args) => accounts_frontiers(rpc_service.node, args).await,
        RpcCommand::WalletFrontiers(args) => wallet_frontiers(rpc_service.node, args).await,
        RpcCommand::Frontiers(args) => frontiers(rpc_service.node, args).await,
        RpcCommand::WalletInfo(args) => wallet_info(rpc_service.node, args).await,
        RpcCommand::WalletExport(args) => wallet_export(args).await,
        RpcCommand::PasswordChange(args) => {
            password_change(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::PasswordEnter(args) => password_enter(rpc_service.node, args).await,
        RpcCommand::PasswordValid(args) => password_valid(rpc_service.node, args).await,
        RpcCommand::DeterministicKey(args) => deterministic_key(args).await,
        RpcCommand::KeyExpand(args) => key_expand(args).await,
        RpcCommand::Peers(args) => peers(rpc_service.node, args).await,
        RpcCommand::PopulateBacklog => populate_backlog(rpc_service.node).await,
        RpcCommand::Representatives(args) => representatives(rpc_service.node, args).await,
        RpcCommand::AccountsRepresentatives(args) => {
            accounts_representatives(rpc_service.node, args).await
        }
        RpcCommand::StatsClear => stats_clear(rpc_service.node).await,
        RpcCommand::UncheckedClear => unchecked_clear(rpc_service.node).await,
        RpcCommand::Unopened(args) => {
            unopened(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::NodeId => node_id(rpc_service.node, rpc_service.enable_control).await,
        RpcCommand::Send(args) => send(rpc_service.node, rpc_service.enable_control, args).await,
        RpcCommand::SearchReceivableAll => {
            search_receivable_all(rpc_service.node, rpc_service.enable_control).await
        }
        RpcCommand::ReceiveMinimum => {
            receive_minimum(rpc_service.node, rpc_service.enable_control).await
        }
        RpcCommand::WalletChangeSeed(args) => {
            wallet_change_seed(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::Delegators(args) => delegators(rpc_service.node, args).await,
        RpcCommand::DelegatorsCount(args) => delegators_count(rpc_service.node, args).await,
        RpcCommand::BlockHash(args) => block_hash(args).await,
        RpcCommand::AccountsBalances(args) => accounts_balances(rpc_service.node, args).await,
        RpcCommand::BlockInfo(args) => block_info(rpc_service.node, args).await,
        RpcCommand::Blocks(args) => blocks(rpc_service.node, args).await,
        RpcCommand::BlocksInfo(args) => blocks_info(rpc_service.node, args).await,
        RpcCommand::Chain(args) => chain(rpc_service.node, args, false).await,
        RpcCommand::Successors(args) => chain(rpc_service.node, args, true).await,
        RpcCommand::ConfirmationActive(args) => confirmation_active(rpc_service.node, args).await,
        RpcCommand::ConfirmationQuorum(args) => confirmation_quorum(rpc_service.node, args).await,
        RpcCommand::WorkValidate(args) => work_validate(rpc_service.node, args).await,
        RpcCommand::AccountInfo(args) => account_info(rpc_service.node, args).await,
        RpcCommand::AccountHistory(args) => account_history(rpc_service.node, args).await,
        RpcCommand::Sign(args) => sign(rpc_service.node, args).await,
        RpcCommand::Process(args) => process(rpc_service.node, args).await,
        RpcCommand::WorkCancel(args) => {
            work_cancel(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::Bootstrap(args) => bootstrap(rpc_service.node, args).await,
        RpcCommand::BootstrapAny(args) => bootstrap_any(rpc_service.node, args).await,
        RpcCommand::BoostrapLazy(args) => bootstrap_lazy(rpc_service.node, args).await,
        RpcCommand::WalletReceivable(args) => {
            wallet_receivable(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletRepresentativeSet(args) => {
            wallet_representative_set(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::SearchReceivable(args) => {
            search_receivable(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletRepublish(args) => {
            wallet_republish(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WalletBalances(args) => wallet_balances(rpc_service.node, args).await,
        RpcCommand::WalletHistory(args) => wallet_history(rpc_service.node, args).await,
        RpcCommand::WalletLedger(args) => {
            wallet_ledger(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::AccountsReceivable(args) => accounts_receivable(rpc_service.node, args).await,
        RpcCommand::Receivable(args) => receivable(rpc_service.node, args).await,
        RpcCommand::ReceivableExists(args) => receivable_exists(rpc_service.node, args).await,
        RpcCommand::RepresentativesOnline(args) => {
            representatives_online(rpc_service.node, args).await
        }
        RpcCommand::Unchecked(args) => unchecked(rpc_service.node, args).await,
        RpcCommand::UncheckedGet(args) => unchecked_get(rpc_service.node, args).await,
        RpcCommand::UncheckedKeys(args) => unchecked_keys(rpc_service.node, args).await,
        RpcCommand::ConfirmationInfo(args) => confirmation_info(rpc_service.node, args).await,
        RpcCommand::Ledger(args) => {
            ledger(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::WorkGenerate(args) => {
            work_generate(rpc_service.node, rpc_service.enable_control, args).await
        }
        RpcCommand::Republish(args) => republish(rpc_service.node, args).await,
        RpcCommand::BlockCreate(args) => {
            block_create(rpc_service.node, rpc_service.enable_control, args).await
        }
        _ => todo!(),
    };

    (StatusCode::OK, to_string_pretty(&response).unwrap()).into_response()
}

async fn set_header<B>(mut request: Request<B>) -> Request<B> {
    request
        .headers_mut()
        .insert("Content-Type", "application/json".parse().unwrap());
    request
}
