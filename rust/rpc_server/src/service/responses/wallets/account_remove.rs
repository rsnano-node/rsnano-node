use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RemovedDto, RpcDto, WalletWithAccountArgs};
use std::sync::Arc;

pub async fn account_remove(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletWithAccountArgs,
) -> RpcDto {
    if enable_control {
        match node.wallets.remove_key(&args.wallet, &args.account.into()) {
            Ok(()) => RpcDto::Removed(RemovedDto::new(true)),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
