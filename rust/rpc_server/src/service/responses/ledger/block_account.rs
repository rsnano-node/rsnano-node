use rsnano_node::Node;
use rsnano_rpc_messages::{AccountRpcMessage, ErrorDto, HashRpcMessage, RpcDto};
use std::sync::Arc;

pub async fn block_account(node: Arc<Node>, args: HashRpcMessage) -> RpcDto {
    let tx = node.ledger.read_txn();
    match &node.ledger.any().get_block(&tx, &args.hash) {
        Some(block) => {
            let account = block.account();
            let block_account = AccountRpcMessage::new(account);
            RpcDto::BlockAccount(block_account)
        }
        None => RpcDto::Error(ErrorDto::BlockNotFound),
    }
}
