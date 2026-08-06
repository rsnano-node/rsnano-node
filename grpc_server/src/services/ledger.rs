use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    FrontierCountRequest, FrontierCountResponse, ReceivableBlocksRequest, ReceivableBlocksResponse,
    ReceivableInfo, ledger_service_server::LedgerService,
};
use rsnano_ledger::AnySet;
use rsnano_node::Node;
use rsnano_types::{Account, BlockHash};
use tonic::{Request, Response, Status};

pub struct LedgerServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl LedgerService for LedgerServiceImpl {
    async fn frontier_count(
        &self,
        _request: Request<FrontierCountRequest>,
    ) -> Result<Response<FrontierCountResponse>, Status> {
        let count = self.node.ledger.account_count();
        Ok(Response::new(FrontierCountResponse { count }))
    }

    async fn receivable_blocks(
        &self,
        request: Request<ReceivableBlocksRequest>,
    ) -> Result<Response<ReceivableBlocksResponse>, Status> {
        let req = request.into_inner();
        let account = Account::parse(&req.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;

        let count = if req.count > 0 {
            req.count as usize
        } else {
            1024
        };

        let any = self.node.ledger.any();
        let mut blocks = std::collections::HashMap::new();

        for (key, info) in any
            .account_receivable_upper_bound(account, BlockHash::ZERO)
            .take(count)
        {
            blocks.insert(
                key.send_block_hash.to_string(),
                ReceivableInfo {
                    amount: info.amount.to_string_dec(),
                    source: info.source.encode_account(),
                },
            );
        }

        Ok(Response::new(ReceivableBlocksResponse { blocks }))
    }
}
