use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    AccountChainState, GetAccountStateRequest, GetAccountStateResponse, ListAccountHistoryRequest,
    ListAccountHistoryResponse, account_service_server::AccountService,
};
use rsnano_ledger::{AnySet, ConfirmedSet, LedgerSet};
use rsnano_node::Node;
use rsnano_types::{Account, BlockHash};
use tonic::{Request, Response, Status};

use crate::block_conversion::{block_record, proto_epoch};
use crate::services::block::decode_hash;

pub struct AccountServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl AccountService for AccountServiceImpl {
    async fn get_account_state(
        &self,
        request: Request<GetAccountStateRequest>,
    ) -> Result<Response<GetAccountStateResponse>, Status> {
        let account = parse_account(&request.into_inner().account)?;
        // Both snapshots come from one ledger read transaction.
        let any = self.node.ledger.any();
        let info = any
            .get_account(&account)
            .ok_or_else(|| Status::not_found("account not found"))?;
        let confirmation = any.confirmed().get_conf_info(&account).unwrap_or_default();

        let current = AccountChainState {
            frontier: info.head.to_string(),
            balance_raw: info.balance.to_string_dec(),
            representative: info.representative.as_account().encode_account(),
            block_count: info.block_count,
            epoch: proto_epoch(info.epoch) as i32,
        };
        let confirmed = if confirmation.frontier.is_zero() {
            None
        } else {
            let block = any
                .get_block(&confirmation.frontier)
                .ok_or_else(|| Status::internal("confirmed frontier is missing"))?;
            let representative_hash = any.representative_block_hash(&confirmation.frontier);
            let representative = any
                .get_block(&representative_hash)
                .and_then(|block| block.representative_field())
                .unwrap_or_default();
            Some(AccountChainState {
                frontier: confirmation.frontier.to_string(),
                balance_raw: block.balance().to_string_dec(),
                representative: representative.as_account().encode_account(),
                block_count: confirmation.height,
                epoch: proto_epoch(block.sideband().details.epoch) as i32,
            })
        };

        Ok(Response::new(GetAccountStateResponse {
            account: account.encode_account(),
            current: Some(current),
            confirmed,
            receivable_raw: any.account_receivable(&account).to_string_dec(),
        }))
    }

    async fn list_account_history(
        &self,
        request: Request<ListAccountHistoryRequest>,
    ) -> Result<Response<ListAccountHistoryResponse>, Status> {
        let request = request.into_inner();
        let account = parse_account(&request.account)?;
        let limit = if request.limit == 0 {
            1024
        } else {
            request.limit as usize
        };
        let any = self.node.ledger.any();
        let info = any
            .get_account(&account)
            .ok_or_else(|| Status::not_found("account not found"))?;
        let mut hash = match request.head {
            Some(value) => decode_hash(&value)?,
            None if request.ascending => info.open_block,
            None => info.head,
        };
        let mut blocks = Vec::new();
        while !hash.is_zero() && blocks.len() < limit {
            let block = any
                .get_block(&hash)
                .ok_or_else(|| Status::not_found("history block not found"))?;
            if block.account() != account {
                return Err(Status::invalid_argument(
                    "history head is not on the requested account chain",
                ));
            }
            let next = if request.ascending {
                any.block_successor(&hash).unwrap_or(BlockHash::ZERO)
            } else {
                block.previous()
            };
            blocks.push(block_record(&self.node.ledger, &block));
            hash = next;
        }
        Ok(Response::new(ListAccountHistoryResponse {
            blocks,
            next: (!hash.is_zero()).then(|| hash.to_string()),
        }))
    }
}

fn parse_account(value: &str) -> Result<Account, Status> {
    Account::parse(value).ok_or_else(|| Status::invalid_argument("invalid account"))
}
