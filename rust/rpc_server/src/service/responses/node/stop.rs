use rsnano_node::{Node, NodeExt};
use rsnano_rpc_messages::{ErrorDto, RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn stop(node: Arc<Node>, enable_control: bool) -> RpcDto {
    if enable_control {
        node.stop();
        RpcDto::Stop(SuccessDto::new())
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
