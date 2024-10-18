use crate::{common::HashRpcMessage, RpcCommand};
use rsnano_core::{Account, Amount, BlockHash, BlockSubType, JsonBlock};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn block_info(hash: BlockHash) -> Self {
        Self::BlockInfo(HashRpcMessage::new(hash))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlockInfoDto {
    pub block_account: Account,
    pub amount: Amount,
    pub balance: Amount,
    pub height: u64,
    pub local_timestamp: u64,
    pub successor: BlockHash,
    pub confirmed: bool,
    pub contents: JsonBlock,
    pub subtype: BlockSubType,
}

impl BlockInfoDto {
    pub fn new(
        block_account: Account,
        amount: Amount,
        balance: Amount,
        height: u64,
        local_timestamp: u64,
        successor: BlockHash,
        confirmed: bool,
        contents: JsonBlock,
        subtype: BlockSubType,
    ) -> Self {
        Self {
            block_account,
            amount,
            balance,
            height,
            local_timestamp,
            successor,
            confirmed,
            contents,
            subtype,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::BlockEnum;
    use serde_json::json;

    #[test]
    fn serialize_block_info_dto() {
        let block_info = BlockInfoDto {
            block_account: Account::decode_account(
                "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
            )
            .unwrap(),
            amount: Amount::raw(30000000000000000000000000000000000u128),
            balance: Amount::raw(5606157000000000000000000000000000000u128),
            height: 58,
            local_timestamp: 0,
            successor: BlockHash::decode_hex(
                "8D3AB98B301224253750D448B4BD997132400CEDD0A8432F775724F2D9821C72",
            )
            .unwrap(),
            confirmed: true,
            contents: BlockEnum::new_test_instance().json_representation(),
            subtype: BlockSubType::Send,
        };

        let serialized = serde_json::to_value(&block_info).unwrap();

        assert_eq!(
            serialized,
            json!({
                "block_account": "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
                "amount": "30000000000000000000000000000000000",
                "balance": "5606157000000000000000000000000000000",
                "height": 58,
                "local_timestamp": 0,
                "successor": "8D3AB98B301224253750D448B4BD997132400CEDD0A8432F775724F2D9821C72",
                "confirmed": true,
                "contents": {
                    "type": "state",
                    "account": "nano_39y535msmkzb31bx73tdnf8iken5ucw9jt98re7nriduus6cgs6uonjdm8r5",
                    "previous": "00000000000000000000000000000000000000000000000000000000000001C8",
                    "representative": "nano_11111111111111111111111111111111111111111111111111ros3kc7wyy",
                    "balance": "420",
                    "link": "000000000000000000000000000000000000000000000000000000000000006F",
                    "link_as_account": "nano_111111111111111111111111111111111111111111111111115hkrzwewgm",
                    "signature": "F26EC6180795C63CFEC46F929DCF6269445208B6C1C837FA64925F1D61C218D4D263F9A73A4B76E3174888C6B842FC1380AC15183FA67E92B2091FEBCCBDB308",
                    "work": "0000000000010F2C"
                  },
                "subtype": "send"
            })
        );
    }

    #[test]
    fn deserialize_block_info_dto() {
        let json = json!({
            "block_account": "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
            "amount": "30000000000000000000000000000000000",
            "balance": "5606157000000000000000000000000000000",
            "height": 58,
            "local_timestamp": 0,
            "successor": "8D3AB98B301224253750D448B4BD997132400CEDD0A8432F775724F2D9821C72",
            "confirmed": true,
            "contents": {
                "type": "state",
                "account": "nano_39y535msmkzb31bx73tdnf8iken5ucw9jt98re7nriduus6cgs6uonjdm8r5",
                "previous": "00000000000000000000000000000000000000000000000000000000000001C8",
                "representative": "nano_11111111111111111111111111111111111111111111111111ros3kc7wyy",
                "balance": "420",
                "link": "000000000000000000000000000000000000000000000000000000000000006F",
                "link_as_account": "nano_111111111111111111111111111111111111111111111111115hkrzwewgm",
                "signature": "F26EC6180795C63CFEC46F929DCF6269445208B6C1C837FA64925F1D61C218D4D263F9A73A4B76E3174888C6B842FC1380AC15183FA67E92B2091FEBCCBDB308",
                "work": "0000000000010F2C"
              },
            "subtype": "send"
        });

        let deserialized: BlockInfoDto = serde_json::from_value(json).unwrap();

        assert_eq!(
            deserialized.block_account,
            Account::decode_account(
                "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est"
            )
            .unwrap()
        );
        assert_eq!(
            deserialized.amount,
            Amount::raw(30000000000000000000000000000000000u128)
        );
        assert_eq!(
            deserialized.balance,
            Amount::raw(5606157000000000000000000000000000000u128)
        );
        assert_eq!(deserialized.height, 58);
        assert_eq!(deserialized.local_timestamp, 0);
        assert_eq!(
            deserialized.successor.to_string(),
            "8D3AB98B301224253750D448B4BD997132400CEDD0A8432F775724F2D9821C72"
        );
        assert!(deserialized.confirmed);
        assert_eq!(deserialized.subtype, BlockSubType::Send);
        assert_eq!(
            deserialized.contents,
            BlockEnum::new_test_instance().json_representation()
        );
    }
}
