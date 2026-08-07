use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    BlockInfoRequest, BlockInfoResponse, BlocksRequest, BlocksResponse, ProcessRequest,
    ProcessResponse, block_service_server::BlockService,
};
use rsnano_ledger::{AnySet, LedgerSet};
use rsnano_node::Node;
use rsnano_types::{BlockHash, BlockType};
use tonic::{Request, Response, Status};

pub struct BlockServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl BlockService for BlockServiceImpl {
    async fn process(
        &self,
        request: Request<ProcessRequest>,
    ) -> Result<Response<ProcessResponse>, Status> {
        let req = request.into_inner();
        let _: rsnano_types::Block = serde_json::from_str(&req.block)
            .map_err(|_| Status::invalid_argument("invalid block JSON"))?;

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
        let detailed = any.detailed_block(&hash);
        let (block, confirmed, amount) = if let Some(d) = detailed {
            (d.block, d.confirmed, d.amount)
        } else if let Some(b) = any.get_block(&hash) {
            let confirmed = any.confirmed().block_exists(&hash);
            let amount = any.block_amount_for(&b);
            (b, confirmed, amount)
        } else {
            return Err(Status::not_found("block not found"));
        };

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

        let subtype = if block.block_type() == BlockType::State {
            block.subtype().as_str().to_string()
        } else {
            String::new()
        };

        let amount_str = amount.map(|a| a.to_string_dec()).unwrap_or_default();

        Ok(Response::new(BlockInfoResponse {
            block_account,
            amount: amount_str,
            balance: block.balance().to_string_dec(),
            height: sideband.height.to_string(),
            local_timestamp: sideband.timestamp.as_u64().to_string(),
            successor,
            confirmed: confirmed.to_string(),
            contents: block.to_json().unwrap_or_default(),
            subtype,
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
