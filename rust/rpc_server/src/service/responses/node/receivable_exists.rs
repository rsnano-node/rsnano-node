use rsnano_core::BlockHash;
use rsnano_node::Node;
use rsnano_rpc_messages::{ExistsDto, ReceivableExistsArgs, RpcDto};
use std::sync::Arc;

pub async fn receivable_exists(node: Arc<Node>, args: ReceivableExistsArgs) -> RpcDto {
    let include_active = args.include_active.unwrap_or(false);
    let include_only_confirmed = args.include_only_confirmed.unwrap_or(true);
    let txn = node.ledger.read_txn();

    let exists = if let Some(block) = node.ledger.get_block(&txn, &args.hash) {
        if block.is_send() {
            let pending_key = rsnano_core::PendingKey::new(block.destination().unwrap(), args.hash);
            let pending_exists = node.ledger.any().get_pending(&txn, &pending_key).is_some();

            if pending_exists {
                block_confirmed(
                    node.clone(),
                    &args.hash,
                    include_active,
                    include_only_confirmed,
                )
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    RpcDto::ReceivableExists(ExistsDto::new(exists))
}

fn block_confirmed(
    node: Arc<Node>,
    hash: &BlockHash,
    include_active: bool,
    include_only_confirmed: bool,
) -> bool {
    let txn = node.ledger.read_txn();

    if include_active && !include_only_confirmed {
        return true;
    }

    if node.ledger.confirmed().block_exists_or_pruned(&txn, hash) {
        return true;
    }

    if !include_only_confirmed {
        if let Some(block) = node.ledger.get_block(&txn, hash) {
            return !node.active.active(&block);
        }
    }

    false
}
