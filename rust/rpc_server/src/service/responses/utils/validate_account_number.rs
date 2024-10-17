use rsnano_rpc_messages::{RpcDto, SuccessDto};

pub async fn validate_account_number() -> RpcDto {
    RpcDto::ValidateAccountNumber(SuccessDto::new())
}
