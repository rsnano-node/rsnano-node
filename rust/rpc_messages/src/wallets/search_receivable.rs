use crate::{common::WalletRpcMessage, RpcCommand};
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn search_receivable(wallet: WalletId) -> Self {
        Self::SearchReceivable(WalletRpcMessage::new(wallet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialize_search_receivable() {
        let command = RpcCommand::search_receivable(WalletId::zero());
        let serialized = serde_json::to_value(&command).unwrap();

        let expected = json!({
            "action": "search_receivable",
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_search_receivable() {
        let json_str = r#"
        {
            "action": "search_receivable",
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        }
        "#;

        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();
        let expected = RpcCommand::search_receivable(WalletId::zero());

        assert_eq!(deserialized, expected);
    }
}
