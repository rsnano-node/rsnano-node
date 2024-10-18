use crate::{common::WalletRpcMessage, RpcCommand};
use rsnano_core::{Account, WalletId, WorkNonce};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

impl RpcCommand {
    pub fn wallet_work_get(wallet: WalletId) -> Self {
        Self::WalletWorkGet(WalletRpcMessage::new(wallet))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountsWithWorkDto {
    pub works: HashMap<Account, WorkNonce>,
}

impl AccountsWithWorkDto {
    pub fn new(works: HashMap<Account, WorkNonce>) -> Self {
        Self { works }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::{Account, WalletId, WorkNonce};
    use serde_json::to_string_pretty;
    use std::collections::HashMap;

    #[test]
    fn serialize_wallet_work_get_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_work_get(WalletId::zero(),)).unwrap(),
            r#"{
  "action": "wallet_work_get",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_wallet_work_get_command() {
        let cmd = RpcCommand::wallet_work_get(WalletId::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn serialize_wallet_work_get_dto() {
        let mut works_map = HashMap::new();
        works_map.insert(Account::zero(), WorkNonce::from(1));

        let works = AccountsWithWorkDto::new(works_map);

        let expected_json = r#"{"works":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":"0000000000000001"}}"#;
        let serialized = serde_json::to_string(&works).unwrap();

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn deserialize_wallet_work_get_dto() {
        let json_data = r#"{"works":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":"0000000000000001"}}"#;
        let works: AccountsWithWorkDto = serde_json::from_str(json_data).unwrap();

        let mut expected_works_map = HashMap::new();
        expected_works_map.insert(Account::zero(), WorkNonce::from(1));

        let expected_works = AccountsWithWorkDto {
            works: expected_works_map,
        };

        assert_eq!(works, expected_works);
    }
}
