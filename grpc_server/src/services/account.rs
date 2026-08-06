use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    AccountBalanceRequest, AccountBalanceResponse, AccountHistoryRequest, AccountHistoryResponse,
    AccountInfoRequest, AccountInfoResponse, AccountRepresentativeRequest,
    AccountRepresentativeResponse, HistoryEntry, account_service_server::AccountService,
};
use rsnano_ledger::{AnySet, LedgerSet};
use rsnano_node::Node;
use rsnano_types::{Account, BlockHash};
use tonic::{Request, Response, Status};

pub struct AccountServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl AccountService for AccountServiceImpl {
    async fn account_info(
        &self,
        request: Request<AccountInfoRequest>,
    ) -> Result<Response<AccountInfoResponse>, Status> {
        let req = request.into_inner();
        let account = Account::parse(&req.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;

        let any = self.node.ledger.any();
        let info = any
            .get_account(&account)
            .ok_or_else(|| Status::not_found("account not found"))?;

        Ok(Response::new(AccountInfoResponse {
            frontier: info.head.to_string(),
            open_block: info.open_block.to_string(),
            representative_block: String::new(),
            balance: info.balance.to_string_dec(),
            modified_timestamp: info.modified.as_u64(),
            block_count: info.block_count,
            account_version: format!("{:?}", info.epoch),
            confirmation_height: "0".to_string(),
            confirmation_height_frontier: String::new(),
        }))
    }

    async fn account_balance(
        &self,
        request: Request<AccountBalanceRequest>,
    ) -> Result<Response<AccountBalanceResponse>, Status> {
        let req = request.into_inner();
        let account = Account::parse(&req.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;

        let any = self.node.ledger.any();
        let balance = any
            .get_account(&account)
            .map(|info| info.balance)
            .unwrap_or_default();

        let receivable = any.account_receivable(&account);

        Ok(Response::new(AccountBalanceResponse {
            balance: balance.to_string_dec(),
            receivable: receivable.to_string_dec(),
        }))
    }

    async fn account_history(
        &self,
        request: Request<AccountHistoryRequest>,
    ) -> Result<Response<AccountHistoryResponse>, Status> {
        let req = request.into_inner();
        let account = Account::parse(&req.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;

        let count = if req.count > 0 {
            req.count as usize
        } else {
            1024
        };

        let any = self.node.ledger.any();
        let mut head = if req.head.is_empty() {
            any.get_account(&account)
                .map(|info| info.head)
                .unwrap_or_default()
        } else {
            BlockHash::decode_hex(&req.head)
                .ok_or_else(|| Status::invalid_argument("invalid head hash"))?
        };

        let mut entries = Vec::new();
        while head != BlockHash::ZERO && entries.len() < count {
            let block = any
                .get_block(&head)
                .ok_or_else(|| Status::not_found("block not found"))?;

            let sideband = block.sideband();
            entries.push(HistoryEntry {
                hash: head.to_string(),
                r#type: format!("{:?}", block.block_type()),
                account: account.encode_account(),
                amount: block.balance().to_string_dec(),
                local_timestamp: "0".to_string(),
                height: sideband.height.to_string(),
            });

            head = any.block_successor(&head).unwrap_or_default();
        }

        let previous = if head != BlockHash::ZERO {
            head.to_string()
        } else {
            "0".to_string()
        };

        Ok(Response::new(AccountHistoryResponse { entries, previous }))
    }

    async fn account_representative(
        &self,
        request: Request<AccountRepresentativeRequest>,
    ) -> Result<Response<AccountRepresentativeResponse>, Status> {
        let req = request.into_inner();
        let account = Account::parse(&req.account)
            .ok_or_else(|| Status::invalid_argument("invalid account"))?;

        let any = self.node.ledger.any();
        let info = any
            .get_account(&account)
            .ok_or_else(|| Status::not_found("account not found"))?;

        Ok(Response::new(AccountRepresentativeResponse {
            representative: info.representative.as_account().encode_account(),
        }))
    }
}
