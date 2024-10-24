use rsnano_core::PendingKey;
use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{BlockDto, ErrorDto, ReceiveArgs, RpcDto};
use std::sync::Arc;

pub async fn receive(node: Arc<Node>, enable_control: bool, args: ReceiveArgs) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let txn = node.ledger.read_txn();

    if !node.ledger.any().block_exists(&txn, &args.block) {
        return RpcDto::Error(ErrorDto::BlockNotFound);
    }

    let pending_info = node
        .ledger
        .any()
        .get_pending(&txn, &PendingKey::new(args.account, args.block));
    if pending_info.is_none() {
        return RpcDto::Error(ErrorDto::BlockNotReceivable);
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
        Ok(_) => RpcDto::Receive(BlockDto::new(block.hash())),
        Err(_) => RpcDto::Error(ErrorDto::ReceiveError),
    }
}
