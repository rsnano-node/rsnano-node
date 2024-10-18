use rsnano_node::Node;
use rsnano_rpc_messages::{RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn populate_backlog(node: Arc<Node>) -> RpcDto {
    node.backlog_population.trigger();
    RpcDto::PopulateBacklog(SuccessDto::new())
}
