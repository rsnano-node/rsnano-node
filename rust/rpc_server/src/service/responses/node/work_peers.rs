use rsnano_node::Node;
use rsnano_rpc_messages::RpcDto;
use std::sync::Arc;

pub async fn work_peers(_node: Arc<Node>, _enable_control: bool) -> RpcDto {
    todo!("Distributed work feature is not implemented yet")
}
