use crate::{common::WalletWithCountArgs, RpcCommand};
use rsnano_core::WalletId;

impl RpcCommand {
    pub fn wallet_republish(wallet: WalletId, count: u64) -> Self {
        Self::WalletRepublish(WalletWithCountArgs::new(wallet, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialize_wallet_republish() {
        let wallet = WalletId::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let command = RpcCommand::wallet_republish(wallet, 2);

        let json = serde_json::to_value(command).unwrap();
        assert_eq!(
            json,
            json!({
                "action": "wallet_republish",
                "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
                "count": 2
            })
        );
    }

    #[test]
    fn deserialize_wallet_republish() {
        let json = json!({
            "action": "wallet_republish",
            "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 2
        });

        let command: RpcCommand = serde_json::from_value(json).unwrap();
        match command {
            RpcCommand::WalletRepublish(args) => {
                assert_eq!(
                    args.wallet,
                    WalletId::decode_hex(
                        "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
                    )
                    .unwrap()
                );
                assert_eq!(args.count, 2);
            }
            _ => panic!("Unexpected RpcCommand variant"),
        }
    }
}
