use rsnano_rpc_messages::{JsonDto, RpcDto, WalletRpcMessage};
use serde_json::{to_string, Value};

pub async fn wallet_export(args: WalletRpcMessage) -> RpcDto {
    RpcDto::WalletExport(JsonDto::new(Value::String(
        to_string(&args.wallet).unwrap(),
    )))
}
