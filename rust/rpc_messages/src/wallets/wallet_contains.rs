use crate::{common::WalletWithAccountArgs, RpcCommand};
use rsnano_core::{Account, WalletId};

impl RpcCommand {
    pub fn wallet_contains(wallet: WalletId, account: Account) -> Self {
        Self::WalletContains(WalletWithAccountArgs::new(wallet, account))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Account;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_balance_command_include_only_confirmed_none() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_contains(1.into(), Account::zero())).unwrap(),
            r#"{
  "action": "wallet_contains",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
}"#
        )
    }

    #[test]
    fn deserialize_wallet_contains_command() {
        let cmd = RpcCommand::wallet_contains(1.into(), Account::zero());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
