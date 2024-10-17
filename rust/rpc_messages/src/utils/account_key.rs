use crate::{common::AccountRpcMessage, RpcCommand};
use rsnano_core::Account;

impl RpcCommand {
    pub fn account_key(account: Account) -> Self {
        Self::AccountKey(AccountRpcMessage::new(account))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Account;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_key_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::account_key(Account::zero())).unwrap(),
            r#"{
  "action": "account_key",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
}"#
        )
    }

    #[test]
    fn deserialize_account_key_command() {
        let cmd = RpcCommand::account_key(Account::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
