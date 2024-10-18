use crate::{common::KeyRpcMessage, RpcCommand};
use rsnano_core::PublicKey;

impl RpcCommand {
    pub fn account_get(key: PublicKey) -> Self {
        Self::AccountGet(KeyRpcMessage::new(key))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::PublicKey;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_get_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::account_get(PublicKey::zero())).unwrap(),
            r#"{
  "action": "account_get",
  "key": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_account_get_command() {
        let cmd = RpcCommand::account_get(PublicKey::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
