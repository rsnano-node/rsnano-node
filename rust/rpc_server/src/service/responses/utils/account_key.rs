use rsnano_rpc_messages::{AccountRpcMessage, KeyRpcMessage, RpcDto};

pub async fn account_key(args: AccountRpcMessage) -> RpcDto {
    RpcDto::AccountKey(KeyRpcMessage::new(args.account.into()))
}
