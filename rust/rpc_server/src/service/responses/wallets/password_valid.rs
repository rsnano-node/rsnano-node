use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, ValidDto, WalletRpcMessage};
use std::sync::Arc;

pub async fn password_valid(node: Arc<Node>, args: WalletRpcMessage) -> RpcDto {
    match node.wallets.valid_password(&args.wallet) {
        Ok(valid) => RpcDto::PasswordValid(ValidDto::new(valid)),
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
