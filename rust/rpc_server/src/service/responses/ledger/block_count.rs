use rsnano_node::Node;
use rsnano_rpc_messages::{BlockCountDto, RpcDto};
use std::sync::Arc;

pub async fn block_count(node: Arc<Node>) -> RpcDto {
    let count = node.ledger.block_count();
    let unchecked = node.unchecked.buffer_count() as u64;
    let cemented = node.ledger.cemented_count();
    let block_count = BlockCountDto::new(count, unchecked, cemented);
    RpcDto::BlockCount(block_count)
}
