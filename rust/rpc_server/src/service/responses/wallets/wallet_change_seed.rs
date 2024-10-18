use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{ErrorDto, RpcDto, WalletChangeSeedArgs, WalletChangeSeedDto};
use std::sync::Arc;

pub async fn wallet_change_seed(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletChangeSeedArgs,
) -> RpcDto {
    if enable_control {
        let (restored_count, last_restored_account) = node
            .wallets
            .change_seed(args.wallet, &args.seed, args.count.unwrap_or(0))
            .unwrap();
        RpcDto::WalletChangeSeed(WalletChangeSeedDto::new(
            last_restored_account,
            restored_count,
        ))
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
