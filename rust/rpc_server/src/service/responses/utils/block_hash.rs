use rsnano_core::BlockEnum;
use rsnano_rpc_messages::{BlockHashArgs, HashRpcMessage, RpcDto};

pub async fn block_hash(args: BlockHashArgs) -> RpcDto {
    let block_enum: BlockEnum = args.block.into();
    RpcDto::BlockHash(HashRpcMessage::new(block_enum.hash()))
}
