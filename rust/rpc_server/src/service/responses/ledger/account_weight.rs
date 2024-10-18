use rsnano_node::Node;
use rsnano_rpc_messages::{AccountWeightArgs, RpcDto, WeightDto};
use std::sync::Arc;

pub async fn account_weight(node: Arc<Node>, args: AccountWeightArgs) -> RpcDto {
    let tx = node.ledger.read_txn();
    let weight = node.ledger.weight_exact(&tx, args.account.into());
    RpcDto::AccountWeight(WeightDto::new(weight))
}
