use rsnano_node::{
    consensus::{ElectionStatus, ElectionStatusType},
    Node,
};
use rsnano_rpc_messages::{ErrorDto, HashRpcMessage, RpcDto, StartedDto};
use std::sync::Arc;

pub async fn block_confirm(node: Arc<Node>, args: HashRpcMessage) -> RpcDto {
    let tx = node.ledger.read_txn();
    match &node.ledger.any().get_block(&tx, &args.hash) {
        Some(block) => {
            if !node
                .ledger
                .confirmed()
                .block_exists_or_pruned(&tx, &args.hash)
            {
                if !node.confirming_set.exists(&args.hash) {
                    node.election_schedulers
                        .manual
                        .push(Arc::new(block.clone()), None);
                }
            } else {
                let mut status = ElectionStatus::default();
                status.winner = Some(Arc::new(block.clone()));
                status.election_end = std::time::SystemTime::now();
                status.block_count = 1;
                status.election_status_type = ElectionStatusType::ActiveConfirmationHeight;
                node.active.insert_recently_cemented(status);
            }
            RpcDto::BlockConfirm(StartedDto::new(true))
        }
        None => RpcDto::Error(ErrorDto::BlockNotFound),
    }
}
