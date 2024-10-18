use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, ExistsDto, RpcDto, WalletWithAccountArgs};
use std::sync::Arc;

pub async fn wallet_contains(node: Arc<Node>, args: WalletWithAccountArgs) -> RpcDto {
    let wallet_accounts = match node.wallets.get_accounts_of_wallet(&args.wallet) {
        Ok(accounts) => accounts,
        Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
    };

    if wallet_accounts.contains(&args.account) {
        RpcDto::Exists(ExistsDto::new(true))
    } else {
        RpcDto::Exists(ExistsDto::new(false))
    }
}
