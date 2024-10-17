use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{ErrorDto, RpcDto, ValidDto, WalletWithPasswordArgs};
use std::sync::Arc;

pub async fn password_enter(node: Arc<Node>, args: WalletWithPasswordArgs) -> RpcDto {
    match node.wallets.enter_password(args.wallet, &args.password) {
        Ok(_) => RpcDto::PasswordEnter(ValidDto::new(true)),
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}
