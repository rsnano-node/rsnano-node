use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{ErrorDto, ExistsDto, RpcDto, WalletRpcMessage};
use std::sync::Arc;

pub async fn search_receivable(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletRpcMessage,
) -> RpcDto {
    if enable_control {
        match node.wallets.search_receivable_wallet(args.wallet) {
            Ok(_) => RpcDto::SearchReceivable(ExistsDto::new(true)),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
