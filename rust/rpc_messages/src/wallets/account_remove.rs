use crate::{common::WalletWithAccountArgs, RpcCommand};
use rsnano_core::{Account, WalletId};

impl RpcCommand {
    pub fn account_remove(wallet: WalletId, account: Account) -> Self {
        Self::AccountRemove(WalletWithAccountArgs::new(wallet, account))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Account;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_remove_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::account_remove(1.into(), Account::zero())).unwrap(),
            r#"{
  "action": "account_remove",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
}"#
        )
    }

    #[test]
    fn deserialize_account_remove_command() {
        let account = Account::from(123);
        let cmd = RpcCommand::account_remove(1.into(), account);
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
