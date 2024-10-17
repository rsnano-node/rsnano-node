use rsnano_node::Node;
use rsnano_rpc_messages::{AccountRepresentativeDto, AccountRpcMessage, ErrorDto, RpcDto};
use std::sync::Arc;

pub async fn account_representative(node: Arc<Node>, args: AccountRpcMessage) -> RpcDto {
    let tx = node.ledger.read_txn();
    match node.ledger.store.account.get(&tx, &args.account) {
        Some(account_info) => RpcDto::AccountRepresentative(AccountRepresentativeDto::new(
            account_info.representative.as_account(),
        )),
        None => RpcDto::Error(ErrorDto::AccountNotFound),
    }
}
