use rsnano_node::Node;
use rsnano_rpc_messages::{AmountRpcMessage, ErrorDto, RpcDto};
use std::sync::Arc;

pub async fn receive_minimum(node: Arc<Node>, enable_control: bool) -> RpcDto {
    if enable_control {
        let amount = node.config.receive_minimum;
        RpcDto::ReceiveMinimum(AmountRpcMessage::new(amount))
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
