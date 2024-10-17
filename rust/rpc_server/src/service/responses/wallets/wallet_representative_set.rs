use rsnano_node::{wallets::WalletsExt, Node};
use rsnano_rpc_messages::{ErrorDto, RpcDto, SetDto, WalletRepresentativeSetArgs};
use std::sync::Arc;

pub async fn wallet_representative_set(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletRepresentativeSetArgs,
) -> RpcDto {
    if enable_control {
        let update_existing = args.update_existing_accounts.unwrap_or(false);
        match node
            .wallets
            .set_representative(args.wallet, args.account.into(), update_existing)
        {
            Ok(_) => RpcDto::WalletRepresentativeSet(SetDto::new(true)),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
