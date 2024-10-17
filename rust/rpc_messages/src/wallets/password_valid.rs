use crate::{common::WalletRpcMessage, RpcCommand};
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn password_valid(wallet: WalletId) -> Self {
        Self::PasswordValid(WalletRpcMessage::new(wallet))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::WalletId;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_password_valid_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::password_valid(WalletId::zero(),)).unwrap(),
            r#"{
  "action": "password_valid",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_password_valid_command() {
        let cmd = RpcCommand::password_valid(WalletId::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
