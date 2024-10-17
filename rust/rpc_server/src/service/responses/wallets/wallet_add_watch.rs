use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, SuccessDto, WalletAddWatchArgs};
use std::sync::Arc;

pub async fn wallet_add_watch(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletAddWatchArgs,
) -> RpcDto {
    if enable_control {
        match node.wallets.insert_watch(&args.wallet, &args.accounts) {
            Ok(_) => RpcDto::WalletAddWatch(SuccessDto::new()),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
