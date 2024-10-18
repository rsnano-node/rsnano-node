use rsnano_core::{BlockDetails, BlockHash, BlockSubType, BlockType};
use rsnano_node::Node;
use rsnano_rpc_messages::{BlockInfoDto, BlocksInfoDto, ErrorDto, HashesArgs, RpcDto};
use std::{collections::HashMap, sync::Arc};

pub async fn blocks_info(node: Arc<Node>, args: HashesArgs) -> RpcDto {
    let txn = node.ledger.read_txn();
    let mut blocks_info: HashMap<BlockHash, BlockInfoDto> = HashMap::new();

    for hash in args.hashes {
        let block = if let Some(block) = node.ledger.get_block(&txn, &hash) {
            block
        } else {
            return RpcDto::Error(ErrorDto::BlockNotFound);
        };

        let account = block.account();
        let amount = node.ledger.any().block_amount(&txn, &hash).unwrap();
        let balance = node.ledger.any().block_balance(&txn, &hash).unwrap();
        let height = block.sideband().unwrap().height;
        let local_timestamp = block.sideband().unwrap().timestamp;
        let successor = block.sideband().unwrap().successor;
        let confirmed = node.ledger.confirmed().block_exists_or_pruned(&txn, &hash);
        let contents = block.json_representation();

        let subtype = match block.block_type() {
            BlockType::State => serde_json::from_str::<BlockSubType>(&BlockDetails::state_subtype(
                &block.sideband().unwrap().details,
            ))
            .unwrap(),
            BlockType::LegacyChange => BlockSubType::Change,
            BlockType::LegacyOpen => BlockSubType::Open,
            BlockType::LegacySend => BlockSubType::Send,
            BlockType::LegacyReceive => BlockSubType::Receive,
            _ => return RpcDto::Error(ErrorDto::BlockError),
        };

        let block_info_dto = BlockInfoDto::new(
            account,
            amount,
            balance,
            height,
            local_timestamp,
            successor,
            confirmed,
            contents,
            subtype,
        );

        blocks_info.insert(hash, block_info_dto);
    }

    RpcDto::BlocksInfo(BlocksInfoDto::new(blocks_info))
}
