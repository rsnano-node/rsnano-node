use crate::{common::WalletWithPasswordArgs, RpcCommand};
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn password_enter(wallet: WalletId, password: String) -> Self {
        Self::PasswordEnter(WalletWithPasswordArgs::new(wallet, password))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::WalletId;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_password_enter_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::password_enter(
                WalletId::zero(),
                "password".to_string()
            ))
            .unwrap(),
            r#"{
  "action": "password_enter",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
  "password": "password"
}"#
        )
    }

    #[test]
    fn deserialize_password_enter_command() {
        let cmd = RpcCommand::password_enter(WalletId::zero(), "password".to_string());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
