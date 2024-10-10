use crate::RpcCommand;
use rsnano_core::BlockHash;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn chain(block: BlockHash, count: u64, offset: Option<u64>, reverse: Option<bool>) -> Self {
        Self::Chain(ChainArgs::new(block, count, offset, reverse))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChainArgs {
    pub block: BlockHash,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
}

impl ChainArgs {
    pub fn new(block: BlockHash, count: u64, offset: Option<u64>, reverse: Option<bool>) -> Self {
        Self {
            block,
            count,
            offset,
            reverse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    #[test]
    fn test_chain_args_serialize() {
        let block_hash = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let chain_args = ChainArgs::new(block_hash, 1, Some(1), Some(false));

        let serialized = serde_json::to_value(chain_args).unwrap();
        let expected = json!({
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 1,
            "offset": 1,
            "reverse": false
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_chain_args() {
        let json_value = json!({
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 1,
            "offset": 1,
            "reverse": false
        });

        let deserialized: ChainArgs = from_value(json_value).unwrap();
        let expected_block_hash = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let expected = ChainArgs::new(expected_block_hash, 1, Some(1), Some(false));

        assert_eq!(deserialized, expected);
    }

    #[test]
    fn serialize_chain_command() {
        let block_hash = BlockHash::decode_hex(
            "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
        )
        .unwrap();
        let chain_command = RpcCommand::chain(block_hash, 1, Some(1), Some(false));

        let serialized = serde_json::to_value(chain_command).unwrap();
        let expected = json!({
            "action": "chain",
            "block": "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F",
            "count": 1,
            "offset": 1,
            "reverse": false
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_chain_command() {
        let json_value = json!({
            "action": "chain",
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
        let expected = RpcCommand::chain(expected_block_hash, 1, Some(1), Some(false));

        assert_eq!(deserialized, expected);
    }
}
