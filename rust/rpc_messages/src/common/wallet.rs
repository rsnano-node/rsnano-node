use rsnano_core::WalletId;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletRpcMessage {
    pub wallet: WalletId,
}

impl WalletRpcMessage {
    pub fn new(wallet: WalletId) -> Self {
        Self { wallet }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn serialize_wallet_rpc_message() {
        let wallet_rpc_message = WalletRpcMessage::new(WalletId::zero());

        let serialized = to_string(&wallet_rpc_message).unwrap();

        let expected_json = serde_json::json!({
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        });

        let actual_json: serde_json::Value = from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn deserialize_wallet_rpc_message() {
        let json_str = r#"{
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        }"#;

        let deserialized: WalletRpcMessage = from_str(json_str).unwrap();

        let expected = WalletRpcMessage::new(WalletId::zero());

        assert_eq!(deserialized, expected);
    }
}
