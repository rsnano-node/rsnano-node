use rsnano_core::WalletId;
use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{ErrorDto, RpcDto, WalletCreateArgs, WalletRpcMessage};
use std::sync::Arc;

pub async fn wallet_create(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletCreateArgs,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let wallet = WalletId::random();
    node.wallets.create(wallet);
    let wallet_create_dto = WalletRpcMessage::new(wallet);

    if let Some(seed) = args.seed {
        node.wallets
            .change_seed(wallet, &seed, 0)
            .expect("This should not fail since the wallet was just created");
    }

    RpcDto::WalletCreate(wallet_create_dto)
}
