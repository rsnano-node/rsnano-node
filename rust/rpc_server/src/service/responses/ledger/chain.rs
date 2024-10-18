use rsnano_core::BlockHash;
use rsnano_node::Node;
use rsnano_rpc_messages::{BlockHashesDto, ChainArgs, RpcDto};
use std::sync::Arc;

pub async fn chain(node: Arc<Node>, args: ChainArgs, successors: bool) -> RpcDto {
    let successors = successors != args.reverse.unwrap_or(false);
    let mut hash = args.block;
    let count = args.count;
    let mut offset = args.offset.unwrap_or(0);
    let mut blocks = Vec::new();

    let txn = node.store.tx_begin_read();

    while !hash.is_zero() && blocks.len() < count as usize {
        if let Some(block) = node.ledger.any().get_block(&txn, &hash) {
            if offset > 0 {
                offset -= 1;
            } else {
                blocks.push(hash);
            }

            hash = if successors {
                node.ledger
                    .any()
                    .block_successor(&txn, &hash)
                    .unwrap_or_else(BlockHash::zero)
            } else {
                block.previous()
            };
        } else {
            hash = BlockHash::zero();
        }
    }

    RpcDto::Chain(BlockHashesDto::new(blocks))
}
