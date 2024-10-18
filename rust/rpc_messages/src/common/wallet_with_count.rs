use rsnano_core::WalletId;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletWithCountArgs {
    pub wallet: WalletId,
    pub count: u64,
}

impl WalletWithCountArgs {
    pub fn new(wallet: WalletId, count: u64) -> Self {
        Self { wallet, count }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn serialize_wallet_with_count_args() {
        let wallet_rpc_message = WalletWithCountArgs::new(WalletId::zero(), 1);

        let serialized = to_string(&wallet_rpc_message).unwrap();

        let expected_json = serde_json::json!({
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "count": 1
        });

        let actual_json: serde_json::Value = from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn deserialize_wallet_with_count_args() {
        let json_str = r#"{
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000",
            "count": 1
        }"#;

        let deserialized: WalletWithCountArgs = from_str(json_str).unwrap();

        let expected = WalletWithCountArgs::new(WalletId::zero(), 1);

        assert_eq!(deserialized, expected);
    }
}
