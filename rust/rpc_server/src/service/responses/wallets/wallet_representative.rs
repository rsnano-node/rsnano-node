use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, WalletRepresentativeDto, WalletRpcMessage};
use std::sync::Arc;

pub async fn wallet_representative(node: Arc<Node>, args: WalletRpcMessage) -> RpcDto {
    match node.wallets.get_representative(args.wallet) {
        Ok(representative) => {
            RpcDto::WalletRepresentative(WalletRepresentativeDto::new(representative.into()))
        }
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
