use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{BlockDto, ErrorDto, RpcDto, SendArgs};
use std::sync::Arc;

pub async fn send(node: Arc<Node>, enable_control: bool, args: SendArgs) -> RpcDto {
    if enable_control {
        let block_hash =
            node.wallets
                .send_sync(args.wallet, args.source, args.destination, args.amount);
        RpcDto::Send(BlockDto::new(block_hash))
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
