use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    FrontierCountRequest, FrontierCountResponse, ListReceivablesRequest, ListReceivablesResponse,
    ReceivableEntry, ReceivableMode, ledger_service_server::LedgerService,
};
use rsnano_ledger::{AnySet, LedgerSet};
use rsnano_node::Node;
use rsnano_types::{Account, Amount, BlockHash};
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
        Ok(Response::new(FrontierCountResponse {
            count: self.node.ledger.account_count(),
        }))
    }

    async fn list_receivables(
        &self,
        request: Request<ListReceivablesRequest>,
    ) -> Result<Response<ListReceivablesResponse>, Status> {
        let request = request.into_inner();
        let account = Account::parse(&request.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;
        let minimum = if request.minimum_amount_raw.is_empty() {
            Amount::ZERO
        } else {
            Amount::decode_dec(&request.minimum_amount_raw)
                .map_err(|_| Status::invalid_argument("invalid minimum_amount_raw"))?
        };
        let mode = ReceivableMode::try_from(request.mode)
            .map_err(|_| Status::invalid_argument("invalid receivable mode"))?;
        let limit = if request.limit == 0 {
            1024
        } else {
            request.limit as usize
        };
        let any = self.node.ledger.any();
        let mut receivables = Vec::new();
        for (key, info) in any.account_receivable_upper_bound(account, BlockHash::ZERO) {
            let cemented = any.confirmed().block_exists(&key.send_block_hash);
            if info.amount < minimum || (mode == ReceivableMode::ConfirmedOnly && !cemented) {
                continue;
            }
            receivables.push(ReceivableEntry {
                send_block_hash: key.send_block_hash.to_string(),
                amount_raw: info.amount.to_string_dec(),
                source_account: info.source.encode_account(),
                cemented,
            });
            if receivables.len() == limit {
                break;
            }
        }
        Ok(Response::new(ListReceivablesResponse { receivables }))
    }
}
