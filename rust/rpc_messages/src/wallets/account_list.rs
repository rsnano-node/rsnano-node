use crate::common::WalletRpcMessage;
use crate::RpcCommand;
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn account_list(wallet: WalletId) -> Self {
        Self::AccountList(WalletRpcMessage::new(wallet))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_list_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::account_list(1.into())).unwrap(),
            r#"{
  "action": "account_list",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001"
}"#
        )
    }

    #[test]
    fn deserialize_account_list_command() {
        let cmd = RpcCommand::account_list(1.into());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
