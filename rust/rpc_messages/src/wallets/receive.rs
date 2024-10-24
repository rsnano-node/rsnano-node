use crate::RpcCommand;
use rsnano_core::{Account, BlockHash, WalletId, WorkNonce};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn receive(args: ReceiveArgs) -> Self {
        Self::Receive(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ReceiveArgs {
    pub wallet: WalletId,
    pub account: Account,
    pub block: BlockHash,
    #[serde(rename = "work", skip_serializing_if = "Option::is_none")]
    pub work: Option<WorkNonce>,
}

impl ReceiveArgs {
    pub fn builder(wallet: WalletId, account: Account, block: BlockHash) -> ReceiveArgsBuilder {
        ReceiveArgsBuilder {
            args: ReceiveArgs {
                wallet,
                account,
                block,
                work: None,
            },
        }
    }
}

pub struct ReceiveArgsBuilder {
    args: ReceiveArgs,
}

impl ReceiveArgsBuilder {
    pub fn set_work(mut self, work: WorkNonce) -> Self {
        self.args.work = Some(work);
        self
    }

    pub fn build(self) -> ReceiveArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialize_receive_args() {
        let wallet = WalletId::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let account = Account::decode_account(
            "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
        )
        .unwrap();
        let block = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();

        let receive_args = ReceiveArgs::builder(wallet, account, block)
            .set_work(1.into())
            .build();

        let serialized = serde_json::to_value(&receive_args).unwrap();
        let expected = json!({
            "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "account": "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "work": "0000000000000001"
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_receive_args() {
        let json_str = r#"{
            "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "account": "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "work": "0000000000000001"
        }"#;

        let deserialized: ReceiveArgs = serde_json::from_str(json_str).unwrap();

        assert_eq!(
            deserialized.wallet,
            WalletId::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
            )
            .unwrap()
        );
        assert_eq!(
            deserialized.account,
            Account::decode_account(
                "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3"
            )
            .unwrap()
        );
        assert_eq!(
            deserialized.block,
            BlockHash::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
            )
            .unwrap()
        );
        assert_eq!(deserialized.work, Some(1.into()));
    }

    #[test]
    fn receive_args_roundtrip() {
        let original = ReceiveArgs {
            wallet: WalletId::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            )
            .unwrap(),
            account: Account::decode_account(
                "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
            )
            .unwrap(),
            block: BlockHash::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            )
            .unwrap(),
            work: Some(1.into()),
        };

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ReceiveArgs = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn serialize_receive_command() {
        let wallet = WalletId::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let account = Account::decode_account(
            "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
        )
        .unwrap();
        let block = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();

        let receive_command = RpcCommand::receive(
            ReceiveArgs::builder(wallet, account, block)
                .set_work(1.into())
                .build(),
        );

        let serialized = serde_json::to_value(&receive_command).unwrap();
        let expected = json!({
            "action": "receive",
            "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "account": "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "work": "0000000000000001"
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_receive_command() {
        let json_str = r#"{
        "action": "receive",
        "wallet": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        "account": "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
        "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        "work": "0000000000000001"
    }"#;

        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();

        match deserialized {
            RpcCommand::Receive(args) => {
                assert_eq!(
                    args.wallet,
                    WalletId::decode_hex(
                        "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
                    )
                    .unwrap()
                );
                assert_eq!(
                    args.account,
                    Account::decode_account(
                        "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3"
                    )
                    .unwrap()
                );
                assert_eq!(
                    args.block,
                    BlockHash::decode_hex(
                        "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
                    )
                    .unwrap()
                );
                assert_eq!(args.work, Some(1.into()));
            }
            _ => panic!("Deserialized to wrong variant"),
        }
    }

    #[test]
    fn receive_command_roundtrip() {
        let wallet = WalletId::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let account = Account::decode_account(
            "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3",
        )
        .unwrap();
        let block = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();

        let original_command = RpcCommand::receive(
            ReceiveArgs::builder(wallet, account, block)
                .set_work(1.into())
                .build(),
        );

        let serialized = serde_json::to_string(&original_command).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original_command, deserialized);
    }
}
