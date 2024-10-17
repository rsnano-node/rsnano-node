use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, NodeIdDto, RpcDto};
use std::sync::Arc;

pub async fn node_id(node: Arc<Node>, enable_control: bool) -> RpcDto {
    if enable_control {
        let private = node.node_id.private_key();
        let public = node.node_id.public_key();
        let as_account = public.as_account();

        RpcDto::NodeId(NodeIdDto::new(private, public, as_account))
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
