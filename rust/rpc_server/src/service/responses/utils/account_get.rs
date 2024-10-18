use rsnano_rpc_messages::{AccountRpcMessage, KeyRpcMessage, RpcDto};

pub async fn account_get(args: KeyRpcMessage) -> RpcDto {
    RpcDto::AccountGet(AccountRpcMessage::new(args.key.into()))
}
