use rsnano_core::Account;
use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{AccountsCreateArgs, AccountsRpcMessage, ErrorDto, RpcDto};
use std::sync::Arc;

pub async fn accounts_create(
    node: Arc<Node>,
    enable_control: bool,
    args: AccountsCreateArgs,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let work = args.work.unwrap_or(true);
    let count = args.wallet_with_count.count as usize;
    let wallet = &args.wallet_with_count.wallet;

    let accounts: Result<Vec<Account>, _> = (0..count)
        .map(|_| node.wallets.deterministic_insert2(wallet, work))
        .map(|result| result.map(|public_key| public_key.into()))
        .collect();

    match accounts {
        Ok(accounts) => RpcDto::Accounts(AccountsRpcMessage::new(accounts)),
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
