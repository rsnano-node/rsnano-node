use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    BlockInfoRequest, BlockInfoResponse, BlocksRequest, BlocksResponse, ProcessRequest,
    ProcessResponse, block_service_server::BlockService,
};
use rsnano_ledger::AnySet;
use rsnano_node::Node;
use rsnano_types::BlockHash;
use tonic::{Request, Response, Status};

pub struct BlockServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl BlockService for BlockServiceImpl {
    async fn process(
        &self,
        _request: Request<ProcessRequest>,
    ) -> Result<Response<ProcessResponse>, Status> {
        Err(Status::unimplemented(
            "block processing not yet implemented",
        ))
    }

    async fn block_info(
        &self,
        request: Request<BlockInfoRequest>,
    ) -> Result<Response<BlockInfoResponse>, Status> {
        let req = request.into_inner();
        let hash = BlockHash::decode_hex(&req.hash)
            .ok_or_else(|| Status::invalid_argument("invalid block hash"))?;

        let any = self.node.ledger.any();
        let block = any
            .get_block(&hash)
            .ok_or_else(|| Status::not_found("block not found"))?;

        let sideband = block.sideband();
        let block_account = any
            .block_account(&hash)
            .map(|a| a.encode_account())
            .unwrap_or_default();

        let successor = any
            .block_successor(&hash)
            .filter(|h| *h != BlockHash::ZERO)
            .map(|h| h.to_string())
            .unwrap_or_else(|| "0".to_string());

        Ok(Response::new(BlockInfoResponse {
            block_account,
            amount: block.balance().to_string_dec(),
            balance: block.balance().to_string_dec(),
            height: sideband.height.to_string(),
            local_timestamp: "0".to_string(),
            successor,
            confirmed: "false".to_string(),
            contents: block.to_json().unwrap_or_default(),
            subtype: format!("{:?}", sideband.details),
        }))
    }

    async fn blocks(
        &self,
        request: Request<BlocksRequest>,
    ) -> Result<Response<BlocksResponse>, Status> {
        let req = request.into_inner();
        let any = self.node.ledger.any();

        let mut blocks_map = std::collections::HashMap::new();
        for hash_str in &req.hashes {
            if let Some(hash) = BlockHash::decode_hex(hash_str) {
                if let Some(block) = any.get_block(&hash) {
                    blocks_map.insert(hash.to_string(), block.to_json().unwrap_or_default());
                } else if req.json_not_found {
                    blocks_map.insert(hash.to_string(), "Block not found".to_string());
                }
            }
        }

        if blocks_map.is_empty() && !req.json_not_found {
            return Err(Status::not_found("block not found"));
        }

        Ok(Response::new(BlocksResponse { blocks: blocks_map }))
    }
}
