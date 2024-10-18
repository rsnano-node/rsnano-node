use rsnano_node::Node;
use rsnano_rpc_messages::{AccountRpcMessage, CountRpcMessage, RpcDto};
use std::sync::Arc;

pub async fn delegators_count(node: Arc<Node>, args: AccountRpcMessage) -> RpcDto {
    let representative = args.account;
    let mut count = 0;

    let tx = node.ledger.read_txn();
    let mut iter = node.store.account.begin(&tx);

    while let Some((_, info)) = iter.current() {
        if info.representative == representative.into() {
            count += 1;
        }

        iter.next();
    }
    RpcDto::DelegatorsCount(CountRpcMessage::new(count))
}
