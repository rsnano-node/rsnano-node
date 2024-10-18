use rsnano_core::{Account, Amount, BlockHash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReceivableDto {
    Blocks {
        blocks: HashMap<Account, Vec<BlockHash>>,
    },
    Threshold {
        blocks: HashMap<Account, HashMap<BlockHash, Amount>>,
    },
    Source {
        blocks: HashMap<Account, HashMap<BlockHash, SourceInfo>>,
    },
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub amount: Amount,
    pub source: Account,
}

impl ReceivableDto {
    pub fn new(blocks: HashMap<Account, Vec<BlockHash>>) -> Self {
        Self::Blocks { blocks }
    }
}

#[cfg(test)]
mod tests {
    use crate::common::{ReceivableDto, SourceInfo};
    use rsnano_core::{Account, Amount, BlockHash};
    use std::collections::HashMap;

    #[test]
    fn serialize_wallet_receivable_dto_blocks() {
        let mut blocks = HashMap::new();
        blocks.insert(Account::zero(), vec![BlockHash::zero()]);

        let works = ReceivableDto::new(blocks);

        let expected_json = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":["0000000000000000000000000000000000000000000000000000000000000000"]}}"#;
        let serialized = serde_json::to_string(&works).unwrap();

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn deserialize_wallet_receivable_dto_blocks() {
        let json_data = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":["0000000000000000000000000000000000000000000000000000000000000000"]}}"#;
        let works: ReceivableDto = serde_json::from_str(json_data).unwrap();

        let mut expected_blocks = HashMap::new();
        expected_blocks.insert(Account::zero(), vec![BlockHash::zero()]);

        let expected_works = ReceivableDto::Blocks {
            blocks: expected_blocks,
        };

        assert_eq!(works, expected_works);
    }

    #[test]
    fn serialize_wallet_receivable_dto_threshold() {
        let mut blocks = HashMap::new();
        let mut inner_map = HashMap::new();
        inner_map.insert(BlockHash::zero(), Amount::from(1000));
        blocks.insert(Account::zero(), inner_map);

        let works = ReceivableDto::Threshold { blocks };

        let expected_json = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":{"0000000000000000000000000000000000000000000000000000000000000000":"1000"}}}"#;
        let serialized = serde_json::to_string(&works).unwrap();

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn deserialize_wallet_receivable_dto_threshold() {
        let json_data = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":{"0000000000000000000000000000000000000000000000000000000000000000":"1000"}}}"#;
        let works: ReceivableDto = serde_json::from_str(json_data).unwrap();

        let mut expected_blocks = HashMap::new();
        let mut inner_map = HashMap::new();
        inner_map.insert(BlockHash::zero(), Amount::from(1000));
        expected_blocks.insert(Account::zero(), inner_map);

        let expected_works = ReceivableDto::Threshold {
            blocks: expected_blocks,
        };

        assert_eq!(works, expected_works);
    }

    #[test]
    fn serialize_wallet_receivable_dto_source() {
        let mut blocks = HashMap::new();
        let mut inner_map = HashMap::new();
        inner_map.insert(
            BlockHash::zero(),
            SourceInfo {
                amount: Amount::from(1000),
                source: Account::zero(),
            },
        );
        blocks.insert(Account::zero(), inner_map);

        let works = ReceivableDto::Source { blocks };

        let expected_json = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":{"0000000000000000000000000000000000000000000000000000000000000000":{"amount":"1000","source":"nano_1111111111111111111111111111111111111111111111111111hifc8npp"}}}}"#;
        let serialized = serde_json::to_string(&works).unwrap();

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn deserialize_wallet_receivable_dto_source() {
        let json_data = r#"{"blocks":{"nano_1111111111111111111111111111111111111111111111111111hifc8npp":{"0000000000000000000000000000000000000000000000000000000000000000":{"amount":"1000","source":"nano_1111111111111111111111111111111111111111111111111111hifc8npp"}}}}"#;
        let works: ReceivableDto = serde_json::from_str(json_data).unwrap();

        let mut expected_blocks = HashMap::new();
        let mut inner_map = HashMap::new();
        inner_map.insert(
            BlockHash::zero(),
            SourceInfo {
                amount: Amount::from(1000),
                source: Account::zero(),
            },
        );
        expected_blocks.insert(Account::zero(), inner_map);

        let expected_works = ReceivableDto::Source {
            blocks: expected_blocks,
        };

        assert_eq!(works, expected_works);
    }
}
