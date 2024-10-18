use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, SuccessDto, WalletWithPasswordArgs};
use std::sync::Arc;

pub async fn password_change(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletWithPasswordArgs,
) -> RpcDto {
    if enable_control {
        match node.wallets.rekey(&args.wallet, args.password) {
            Ok(_) => RpcDto::PasswordChange(SuccessDto::new()),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
