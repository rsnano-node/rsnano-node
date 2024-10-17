use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, SuccessDto, WorkSetArgs};
use std::sync::Arc;

pub async fn work_set(node: Arc<Node>, enable_control: bool, args: WorkSetArgs) -> RpcDto {
    if enable_control {
        match node
            .wallets
            .work_set(&args.wallet, &args.account.into(), args.work.into())
        {
            Ok(_) => RpcDto::WorkSet(SuccessDto::new()),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
