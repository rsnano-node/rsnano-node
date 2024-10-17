use rsnano_core::{BlockHash, JsonBlock};
use rsnano_node::Node;
use rsnano_rpc_messages::{BlocksDto, HashesArgs, RpcDto};
use std::{collections::HashMap, sync::Arc};

pub async fn blocks(node: Arc<Node>, args: HashesArgs) -> RpcDto {
    let mut blocks: HashMap<BlockHash, JsonBlock> = HashMap::new();
    let txn = node.ledger.read_txn();
    for hash in args.hashes {
        let block = node.ledger.get_block(&txn, &hash).unwrap();
        blocks.insert(hash, block.json_representation());
    }
    RpcDto::Blocks(BlocksDto::new(blocks))
}
