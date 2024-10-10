use super::ChainArgs;
use crate::RpcCommand;
use rsnano_core::BlockHash;

impl RpcCommand {
    pub fn successors(
        block: BlockHash,
        count: u64,
        offset: Option<u64>,
        reverse: Option<bool>,
    ) -> Self {
        Self::Successors(ChainArgs::new(block, count, offset, reverse))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    #[test]
    fn serialize_successors_command() {
        let block_hash = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let successors_command = RpcCommand::successors(block_hash, 1, Some(1), Some(false));

        let serialized = serde_json::to_value(successors_command).unwrap();
        let expected = json!({
            "action": "successors",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 1,
            "offset": 1,
            "reverse": false
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_successors_command() {
        let json_value = json!({
            "action": "successors",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 1,
            "offset": 1,
            "reverse": false
        });

        let deserialized: RpcCommand = from_value(json_value).unwrap();
        let expected_block_hash = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let expected = RpcCommand::successors(expected_block_hash, 1, Some(1), Some(false));

        assert_eq!(deserialized, expected);
    }
}
