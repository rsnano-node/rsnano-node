use rsnano_node::Node;
use rsnano_rpc_messages::{RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn stats_clear(node: Arc<Node>) -> RpcDto {
    node.stats.clear();
    RpcDto::StatsClear(SuccessDto::new())
}
