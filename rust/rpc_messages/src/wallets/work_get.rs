use crate::common::WalletWithAccountArgs;
use crate::RpcCommand;
use rsnano_core::WorkNonce;
use rsnano_core::{Account, WalletId};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn work_get(wallet: WalletId, account: Account) -> Self {
        Self::WorkGet(WalletWithAccountArgs::new(wallet, account))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorkDto {
    pub work: WorkNonce,
}

impl WorkDto {
    pub fn new(work: WorkNonce) -> Self {
        Self { work }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::{Account, WalletId};
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_work_get_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::work_get(WalletId::zero(), Account::zero())).unwrap(),
            r#"{
  "action": "work_get",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
}"#
        )
    }

    #[test]
    fn deserialize_work_get_command() {
        let cmd = RpcCommand::work_get(WalletId::zero(), Account::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn serialize_work_get_dto() {
        let work = WorkDto::new(1.into());

        let expected_json = r#"{"work":"0000000000000001"}"#;
        let serialized = serde_json::to_string(&work).unwrap();

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn deserialize_work_get_dto() {
        let json_data = r#"{"work":"0000000000000001"}"#;
        let work: WorkDto = serde_json::from_str(json_data).unwrap();

        let expected_work = WorkDto::new(1.into());

        assert_eq!(work, expected_work);
    }
}
