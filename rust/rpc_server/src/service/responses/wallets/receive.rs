use rsnano_core::PendingKey;
use rsnano_node::{Node, wallets::WalletsExt};
use rsnano_rpc_messages::{BlockHashRpcMessage, ErrorDto, ReceiveArgs};
use serde_json::to_string_pretty;
use std::sync::Arc;

pub async fn receive(node: Arc<Node>, enable_control: bool, args: ReceiveArgs) -> String {
    if !enable_control {
        return to_string_pretty(&ErrorDto::new("RPC control is disabled".to_string())).unwrap();
    }

    let txn = node.ledger.read_txn();

    if !node.ledger.any().block_exists(&txn, &args.block) {
        return to_string_pretty(&ErrorDto::new("Block not found".to_string())).unwrap();
    }

    let pending_info = node
        .ledger
        .any()
        .get_pending(&txn, &PendingKey::new(args.account, args.block));
    if pending_info.is_none() {
        return to_string_pretty(&ErrorDto::new("Block is not receivable".to_string())).unwrap();
    }

    let representative = node
        .wallets
        .get_representative(args.wallet)
        .unwrap_or_default();

    let wallets = node.wallets.mutex.lock().unwrap();
    let wallet = wallets.get(&args.wallet).unwrap().to_owned();

    let block = node
        .ledger
        .any()
        .get_block(&node.ledger.read_txn(), &args.block)
        .unwrap();

    let receive =
        node.wallets
            .receive_sync(wallet, &block, representative, node.config.receive_minimum);

    match receive {
        Ok(_) => {
            to_string_pretty(&BlockHashRpcMessage::new("block".to_string(), block.hash())).unwrap()
        }
        Err(_) => to_string_pretty(&ErrorDto::new("Receive error".to_string())).unwrap(),
    }
}
