use rsnano_core::{Amount, MXRB_RATIO};
use rsnano_rpc_messages::{AmountRpcMessage, RpcDto};

pub async fn raw_to_nano(args: AmountRpcMessage) -> RpcDto {
    RpcDto::RawToNano(AmountRpcMessage::new(Amount::nano(
        args.amount.number() / *MXRB_RATIO,
    )))
}
