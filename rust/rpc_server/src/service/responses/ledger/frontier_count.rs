use rsnano_node::Node;
use rsnano_rpc_messages::{CountRpcMessage, RpcDto};
use std::sync::Arc;

pub async fn frontier_count(node: Arc<Node>) -> RpcDto {
    RpcDto::FrontierCount(CountRpcMessage::new(node.ledger.account_count()))
}
