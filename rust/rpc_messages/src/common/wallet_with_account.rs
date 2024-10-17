use rsnano_core::{Account, WalletId};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletWithAccountArgs {
    pub wallet: WalletId,
    pub account: Account,
}

impl WalletWithAccountArgs {
    pub fn new(wallet: WalletId, account: Account) -> Self {
        Self { wallet, account }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn serialize_wallet_with_account_args() {
        let wallet_rpc_message = WalletWithAccountArgs::new(WalletId::zero(), Account::zero());

        let serialized = to_string(&wallet_rpc_message).unwrap();

        let expected_json = serde_json::json!({
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
        });

        let actual_json: serde_json::Value = from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn deserialize_wallet_with_account_args() {
        let json_str = r#"{
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
        }"#;

        let deserialized: WalletWithAccountArgs = from_str(json_str).unwrap();

        let expected = WalletWithAccountArgs::new(WalletId::zero(), Account::zero());

        assert_eq!(deserialized, expected);
    }
}
