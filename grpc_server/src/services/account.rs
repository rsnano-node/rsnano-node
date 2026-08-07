use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    AccountBalanceRequest, AccountBalanceResponse, AccountHistoryRequest, AccountHistoryResponse,
    AccountInfoRequest, AccountInfoResponse, AccountRepresentativeRequest,
    AccountRepresentativeResponse, HistoryEntry, account_service_server::AccountService,
};
use rsnano_ledger::{AnySet, ConfirmedSet, LedgerSet};
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

        let conf_info = any
            .confirmed()
            .get_conf_info(&account)
            .unwrap_or_default();

        Ok(Response::new(AccountInfoResponse {
            frontier: info.head.to_string(),
            open_block: info.open_block.to_string(),
            representative_block: any.representative_block_hash(&info.head).to_string(),
            balance: info.balance.to_string_dec(),
            modified_timestamp: info.modified.as_u64(),
            block_count: info.block_count,
            account_version: format!("{:?}", info.epoch),
            confirmation_height: conf_info.height.to_string(),
            confirmation_height_frontier: conf_info.frontier.to_string(),
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
                .map(|info| {
                    if req.reverse {
                        info.open_block
                    } else {
                        info.head
                    }
                })
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
            let (entry_type, counterparty_account) = match &*block {
                rsnano_types::Block::LegacySend(b) => ("send", b.destination()),
                rsnano_types::Block::LegacyReceive(b) => {
                    let source_acc = any.block_account(&b.source()).unwrap_or_default();
                    ("receive", source_acc)
                }
                rsnano_types::Block::LegacyOpen(b) => {
                    let genesis = self.node.network_params.ledger.genesis_account;
                    let source_acc = if b.source() == genesis.into() {
                        genesis
                    } else {
                        any.block_account(&b.source()).unwrap_or_default()
                    };
                    ("receive", source_acc)
                }
                rsnano_types::Block::LegacyChange(b) => ("change", b.mandatory_representative().into()),
                rsnano_types::Block::State(_) => {
                    let subtype_str = block.subtype().as_str();
                    let counterparty = if block.is_send() {
                        block.destination().unwrap_or_default()
                    } else if block.is_receive() {
                        block.source().and_then(|s| any.block_account(&s)).unwrap_or_default()
                    } else {
                        account
                    };
                    (subtype_str, counterparty)
                }
            };

            let amount = any.block_amount_for(&block).unwrap_or_default();

            entries.push(HistoryEntry {
                hash: head.to_string(),
                r#type: entry_type.to_string(),
                account: counterparty_account.encode_account(),
                amount: amount.to_string_dec(),
                local_timestamp: sideband.timestamp.as_u64().to_string(),
                height: sideband.height.to_string(),
            });

            head = if req.reverse {
                any.block_successor(&head).unwrap_or_default()
            } else {
                block.previous()
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_ledger::{DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH};
    use rsnano_types::Amount;
    use test_helpers::{System, process_send_block};

    #[test]
    fn account_history_walks_previous_blocks_by_default() {
        let mut system = System::new();
        let node = system.make_node();
        let first = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));
        let second = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));

        let response = account_history(&node, "", false, 3);

        let hashes: Vec<_> = response
            .entries
            .iter()
            .map(|entry| entry.hash.clone())
            .collect();
        assert_eq!(
            hashes,
            vec![
                second.hash().to_string(),
                first.hash().to_string(),
                DEV_GENESIS_HASH.to_string(),
            ]
        );
    }

    #[test]
    fn reverse_account_history_walks_successors_from_open_block() {
        let mut system = System::new();
        let node = system.make_node();
        let first = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));
        let second = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));

        let response = account_history(&node, "", true, 3);

        let hashes: Vec<_> = response
            .entries
            .iter()
            .map(|entry| entry.hash.clone())
            .collect();
        assert_eq!(
            hashes,
            vec![
                DEV_GENESIS_HASH.to_string(),
                first.hash().to_string(),
                second.hash().to_string(),
            ]
        );
    }

    #[test]
    fn explicit_history_head_is_respected_in_both_directions() {
        let mut system = System::new();
        let node = system.make_node();
        let first = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));
        let second = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));

        let backward = account_history(&node, &first.hash().to_string(), false, 2);
        let forward = account_history(&node, &first.hash().to_string(), true, 2);

        assert_eq!(backward.entries[1].hash, DEV_GENESIS_HASH.to_string());
        assert_eq!(forward.entries[1].hash, second.hash().to_string());
    }

    fn account_history(
        node: &Arc<Node>,
        head: &str,
        reverse: bool,
        count: i32,
    ) -> AccountHistoryResponse {
        let service = AccountServiceImpl { node: node.clone() };
        node.runtime
            .block_on(service.account_history(Request::new(AccountHistoryRequest {
                account: DEV_GENESIS_ACCOUNT.encode_account(),
                head: head.to_string(),
                count,
                raw: false,
                reverse,
                account_filter: String::new(),
            })))
            .unwrap()
            .into_inner()
    }
}
