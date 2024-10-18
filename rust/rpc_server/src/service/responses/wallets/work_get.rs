use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, WalletWithAccountArgs, WorkDto};
use std::sync::Arc;

pub async fn work_get(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletWithAccountArgs,
) -> RpcDto {
    if enable_control {
        match node.wallets.work_get2(&args.wallet, &args.account.into()) {
            Ok(work) => RpcDto::WorkGet(WorkDto::new(work.into())),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
