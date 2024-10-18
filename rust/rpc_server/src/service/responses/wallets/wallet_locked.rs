use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, LockedDto, RpcDto, WalletRpcMessage};
use std::sync::Arc;

pub async fn wallet_locked(node: Arc<Node>, args: WalletRpcMessage) -> RpcDto {
    match node.wallets.valid_password(&args.wallet) {
        Ok(valid) => RpcDto::Locked(LockedDto::new(!valid)),
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
