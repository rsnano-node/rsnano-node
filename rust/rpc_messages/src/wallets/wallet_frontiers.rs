use crate::{common::WalletRpcMessage, RpcCommand};
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn wallet_frontiers(wallet: WalletId) -> Self {
        Self::WalletFrontiers(WalletRpcMessage::new(wallet))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::WalletId;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_wallet_frontiers_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_frontiers(WalletId::zero())).unwrap(),
            r#"{
  "action": "wallet_frontiers",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_wallet_frontiers_command() {
        let json_str = r#"{
    "action": "wallet_frontiers",
    "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
    }"#;
        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();
        let expected_command = RpcCommand::wallet_frontiers(WalletId::zero());
        assert_eq!(deserialized, expected_command);
    }
}
