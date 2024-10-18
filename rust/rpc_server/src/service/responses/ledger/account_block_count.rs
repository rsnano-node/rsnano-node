use rsnano_node::Node;
use rsnano_rpc_messages::{AccountBlockCountDto, AccountRpcMessage, ErrorDto, RpcDto};
use std::sync::Arc;

pub async fn account_block_count(node: Arc<Node>, args: AccountRpcMessage) -> RpcDto {
    let tx = node.ledger.read_txn();
    match node.ledger.store.account.get(&tx, &args.account) {
        Some(account_info) => {
            RpcDto::AccountBlockCount(AccountBlockCountDto::new(account_info.block_count))
        }
        None => RpcDto::Error(ErrorDto::AccountNotFound),
    }
}
