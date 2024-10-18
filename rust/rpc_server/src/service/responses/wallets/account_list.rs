use rsnano_node::Node;
use rsnano_rpc_messages::{AccountsRpcMessage, ErrorDto, RpcDto, WalletRpcMessage};
use std::sync::Arc;

pub async fn account_list(node: Arc<Node>, args: WalletRpcMessage) -> RpcDto {
    match node.wallets.get_accounts_of_wallet(&args.wallet) {
        Ok(accounts) => RpcDto::Accounts(AccountsRpcMessage::new(accounts)),
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
