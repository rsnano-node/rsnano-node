use rsnano_node::Node;
use rsnano_rpc_messages::{AddressWithPortArgs, RpcDto};
use std::sync::Arc;

pub async fn work_peer_add(
    _node: Arc<Node>,
    _enable_control: bool,
    _args: AddressWithPortArgs,
) -> RpcDto {
    todo!("Distributed work feature is not implemented yet")
}
