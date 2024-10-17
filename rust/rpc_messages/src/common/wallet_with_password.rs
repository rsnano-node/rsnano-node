use rsnano_core::WalletId;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletWithPasswordArgs {
    pub wallet: WalletId,
    pub password: String,
}

impl WalletWithPasswordArgs {
    pub fn new(wallet: WalletId, password: String) -> Self {
        Self { wallet, password }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn serialize_wallet_with_password_args() {
        let wallet_rpc_message =
            WalletWithPasswordArgs::new(WalletId::zero(), "password".to_string());

        let serialized = to_string(&wallet_rpc_message).unwrap();

        let expected_json = serde_json::json!({
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "password": "password"
        });

        let actual_json: serde_json::Value = from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn deserialize_wallet_with_password_args() {
        let json_str = r#"{
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "password": "password"
        }"#;

        let deserialized: WalletWithPasswordArgs = from_str(json_str).unwrap();

        let expected = WalletWithPasswordArgs::new(WalletId::zero(), "password".to_string());

        assert_eq!(deserialized, expected);
    }
}
