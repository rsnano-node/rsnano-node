use rsnano_node::Node;
use rsnano_rpc_messages::{RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn unchecked_clear(node: Arc<Node>) -> RpcDto {
    node.unchecked.clear();
    RpcDto::UncheckedClear(SuccessDto::new())
}
