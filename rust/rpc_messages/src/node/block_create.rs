use crate::{BlockTypeDto, RpcCommand, WorkVersionDto};
use rsnano_core::{Account, Amount, BlockHash, JsonBlock, Link, RawKey, WalletId, WorkNonce};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn block_create(block_create_args: BlockCreateArgs) -> Self {
        Self::BlockCreate(block_create_args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlockCreateArgs {
    #[serde(rename = "type")]
    pub block_type: BlockTypeDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<RawKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet: Option<WalletId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<BlockHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative: Option<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<BlockHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<WorkNonce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<WorkVersionDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u64>,
}

impl BlockCreateArgs {
    pub fn new(
        block_type: BlockTypeDto,
        balance: Option<Amount>,
        key: Option<RawKey>,
        wallet: Option<WalletId>,
        account: Option<Account>,
        source: Option<BlockHash>,
        destination: Option<Account>,
        representative: Option<Account>,
        link: Option<Link>,
        previous: Option<BlockHash>,
        work: Option<WorkNonce>,
        version: Option<WorkVersionDto>,
        difficulty: Option<u64>,
    ) -> Self {
        Self {
            block_type,
            balance,
            key,
            wallet,
            account,
            source,
            destination,
            representative,
            link,
            previous,
            work,
            version,
            difficulty,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlockCreateDto {
    pub hash: BlockHash,
    pub difficulty: WorkNonce,
    pub block: JsonBlock,
}

impl BlockCreateDto {
    pub fn new(hash: BlockHash, difficulty: WorkNonce, block: JsonBlock) -> Self {
        Self {
            hash,
            difficulty,
            block,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::{Block, KeyPair, RawKey, StateBlock};
    use serde_json::json;

    #[test]
    fn serialize_block_create_command() {
        // Create a test StateBlock instance
        let state_block = StateBlock::new_test_instance();
        let key_pair = KeyPair::new();
        let raw_key = RawKey::from(key_pair.private_key());

        // Create BlockCreateArgs using the test StateBlock data
        let block_create_args = BlockCreateArgs {
            block_type: BlockTypeDto::State,
            balance: Some(state_block.balance()),
            key: Some(raw_key),
            wallet: None,
            account: Some(state_block.account()),
            source: None,
            destination: None,
            representative: Some(state_block.mandatory_representative().as_account()),
            link: Some(state_block.link()),
            previous: Some(state_block.previous()),
            work: Some(WorkNonce::from(state_block.work())),
            version: Some(WorkVersionDto::Work1),
            difficulty: None,
        };

        // Create the RpcCommand
        let command = RpcCommand::block_create(block_create_args);

        // Serialize the command to JSON
        let serialized = serde_json::to_string_pretty(&command).unwrap();

        // Expected JSON
        let expected_json = json!({
            "action": "block_create",
            "type": "state",
            "balance": state_block.balance().to_string_dec(),
            "key": raw_key.encode_hex(),
            "account": state_block.account().encode_account(),
            "representative": state_block.mandatory_representative().as_account(),
            "link": state_block.link().encode_hex(),
            "previous": state_block.previous().encode_hex(),
            "work": format!("{:016X}", state_block.work()),
            "version": "work1"
        });

        // Deserialize the expected JSON to a Value
        let expected_value: serde_json::Value = expected_json;

        // Deserialize the serialized command to a Value
        let serialized_value: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Assert that the serialized command matches the expected JSON
        assert_eq!(serialized_value, expected_value);
    }

    #[test]
    fn deserialize_block_create_command() {
        // Create a test StateBlock instance
        let state_block = StateBlock::new_test_instance();
        let key_pair = KeyPair::new();
        let raw_key = RawKey::from(key_pair.private_key());

        // Create JSON representation
        let json = json!({
            "action": "block_create",
            "type": "state",
            "balance": state_block.balance().to_string_dec(),
            "key": raw_key.encode_hex(),
            "account": state_block.account().encode_account(),
            "representative": state_block.mandatory_representative().as_account(),
            "link": state_block.link().encode_hex(),
            "previous": state_block.previous().encode_hex(),
            "work": format!("{:016X}", state_block.work()),
            "version": "work1"
        });

        // Serialize JSON to string
        let json_string = serde_json::to_string(&json).unwrap();

        // Deserialize the string to RpcCommand
        let command: RpcCommand = serde_json::from_str(&json_string).unwrap();

        // Expected BlockCreateArgs
        let expected_args = BlockCreateArgs {
            block_type: BlockTypeDto::State,
            balance: Some(state_block.balance()),
            key: Some(raw_key),
            wallet: None,
            account: Some(state_block.account()),
            source: None,
            destination: None,
            representative: Some(state_block.representative_field().unwrap().into()),
            link: Some(state_block.link()),
            previous: Some(state_block.previous()),
            work: Some(WorkNonce::from(state_block.work())),
            version: Some(WorkVersionDto::Work1),
            difficulty: None,
        };

        // Expected command
        let expected_command = RpcCommand::block_create(expected_args);

        // Assert that the deserialized command matches the expected command
        assert_eq!(command, expected_command);
    }

    #[test]
    fn serialize_block_create_dto() {
        let block = StateBlock::new_test_instance();

        let dto = BlockCreateDto::new(block.hash(), 10.into(), block.json_representation());

        let serialized = serde_json::to_string_pretty(&dto).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        let expected_json = json!({
            "hash": block.hash(),
            "difficulty": "000000000000000A",
            "block": block.json_representation()
        });

        assert_eq!(deserialized, expected_json);
    }

    #[test]
    fn deserialize_block_create_dto() {
        let block = StateBlock::new_test_instance();

        let json = json!({
            "hash": block.hash(),
            "difficulty": "000000000000000A",
            "block": block.json_representation()
        });

        let json_string = serde_json::to_string(&json).unwrap();

        let dto: BlockCreateDto = serde_json::from_str(&json_string).unwrap();

        assert_eq!(dto.hash, block.hash());
        assert_eq!(dto.difficulty, 10.into());
        assert_eq!(dto.block, block.json_representation());
    }
}
