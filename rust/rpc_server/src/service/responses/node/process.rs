use rsnano_core::BlockEnum;
use rsnano_ledger::BlockStatus;
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, HashRpcMessage, ProcessArgs, RpcDto};
use std::sync::Arc;

pub async fn process(node: Arc<Node>, args: ProcessArgs) -> RpcDto {
    let is_async = args.is_async.unwrap_or(false);
    let block: BlockEnum = args.block.into();

    if node
        .network_params
        .work
        .validate_entry(block.work_version(), &block.root(), block.work())
    {
        return RpcDto::Error(ErrorDto::WorkLow);
    }

    if !is_async {
        match node.process_local(block.clone()) {
            Some(result) => match result {
                BlockStatus::Progress => {
                    let hash = block.hash();
                    RpcDto::Process(HashRpcMessage::new(hash))
                }
                BlockStatus::GapPrevious => RpcDto::Error(ErrorDto::GapPrevious),
                BlockStatus::GapSource => RpcDto::Error(ErrorDto::GapSource),
                BlockStatus::Old => RpcDto::Error(ErrorDto::Old),
                BlockStatus::BadSignature => RpcDto::Error(ErrorDto::BadSignature),
                BlockStatus::NegativeSpend => RpcDto::Error(ErrorDto::NegativeSpend),
                BlockStatus::BalanceMismatch => RpcDto::Error(ErrorDto::BalanceMismatch),
                BlockStatus::Unreceivable => RpcDto::Error(ErrorDto::Unreceivable),
                BlockStatus::BlockPosition => RpcDto::Error(ErrorDto::BlockPosition),
                BlockStatus::GapEpochOpenPending => RpcDto::Error(ErrorDto::GapEpochOpenPending),
                BlockStatus::Fork => {
                    if args.force.unwrap_or(false) {
                        node.active.erase(&block.qualified_root());
                        node.block_processor.force(Arc::new(block.clone()));
                        let hash = block.hash();
                        RpcDto::Process(HashRpcMessage::new(hash))
                    } else {
                        RpcDto::Error(ErrorDto::Fork)
                    }
                }
                BlockStatus::InsufficientWork => RpcDto::Error(ErrorDto::InsufficientWork),
                BlockStatus::OpenedBurnAccount => RpcDto::Error(ErrorDto::OpenedBurnAccount),
                _ => RpcDto::Error(ErrorDto::Other),
            },
            None => RpcDto::Error(ErrorDto::Stopped),
        }
    } else {
        if let BlockEnum::State(_) = block {
            node.process(block.clone()).unwrap(); // TODO add error handling!
            RpcDto::Process(HashRpcMessage::new(block.hash()))
        } else {
            RpcDto::Error(ErrorDto::BlockError)
        }
    }
}
