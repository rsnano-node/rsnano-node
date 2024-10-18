use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, FrontiersDto, RpcDto, WalletRpcMessage};
use std::{collections::HashMap, sync::Arc};

pub async fn wallet_frontiers(node: Arc<Node>, args: WalletRpcMessage) -> RpcDto {
    let tx = node.ledger.read_txn();
    let mut frontiers = HashMap::new();

    let accounts = match node.wallets.get_accounts_of_wallet(&args.wallet) {
        Ok(accounts) => accounts,
        Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
    };

    for account in accounts {
        if let Some(block_hash) = node.ledger.any().account_head(&tx, &account) {
            frontiers.insert(account, block_hash);
        }
    }
    RpcDto::WalletFrontiers(FrontiersDto::new(frontiers))
}
