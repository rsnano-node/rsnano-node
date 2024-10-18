use rsnano_core::PublicKey;
use rsnano_node::Node;
use rsnano_rpc_messages::{AccountMoveArgs, ErrorDto, MovedDto, RpcDto};
use std::sync::Arc;

pub async fn account_move(node: Arc<Node>, enable_control: bool, args: AccountMoveArgs) -> RpcDto {
    if enable_control {
        let public_keys: Vec<PublicKey> =
            args.accounts.iter().map(|account| account.into()).collect();
        let result = node
            .wallets
            .move_accounts(&args.source, &args.wallet, &public_keys);

        match result {
            Ok(()) => RpcDto::Moved(MovedDto::new(true)),
            Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
