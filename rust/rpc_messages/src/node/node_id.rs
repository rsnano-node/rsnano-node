use crate::RpcCommand;

impl RpcCommand {
    pub fn node_id() -> Self {
        Self::NodeId
    }
}

use rsnano_core::{Account, PublicKey, RawKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NodeIdDto {
    pub private: RawKey,
    pub public: PublicKey,
    pub as_account: Account,
    #[serde(
        serialize_with = "serialize_node_id",
        deserialize_with = "deserialize_node_id"
    )]
    pub node_id: Account,
}

impl NodeIdDto {
    pub fn new(private: RawKey, public: PublicKey, as_account: Account) -> Self {
        Self {
            private,
            public,
            as_account,
            node_id: as_account,
        }
    }
}

fn serialize_node_id<S>(account: &Account, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&account.to_node_id())
}

fn deserialize_node_id<'de, D>(deserializer: D) -> Result<Account, D::Error>
where
    D: Deserializer<'de>,
{
    let node_id_str = String::deserialize(deserializer)?;
    let account_str = node_id_str.replacen("node", "nano", 1);
    Account::decode_account(&account_str).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialize_node_id_command() {
        let command = RpcCommand::node_id();
        let serialized = serde_json::to_value(&command).unwrap();
        let expected = json!({
            "action": "node_id"
        });
        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_node_id_command() {
        let json_str = r#"{"action": "node_id"}"#;
        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();
        assert!(matches!(deserialized, RpcCommand::NodeId));
    }

    #[test]
    fn serialize_node_id_dto() {
        let node_id_dto = NodeIdDto {
            private: RawKey::zero(),
            public: PublicKey::zero(),
            as_account: Account::zero(),
            node_id: Account::zero(),
        };

        let serialized = serde_json::to_value(&node_id_dto).unwrap();
        let expected = json!({
            "private": "0000000000000000000000000000000000000000000000000000000000000000",
            "public": "0000000000000000000000000000000000000000000000000000000000000000",
            "as_account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp",
            "node_id": "node_1111111111111111111111111111111111111111111111111111hifc8npp"
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_node_id_dto() {
        let json_str = r#"{
            "private": "0000000000000000000000000000000000000000000000000000000000000000",
            "public": "0000000000000000000000000000000000000000000000000000000000000000",
            "as_account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp",
            "node_id": "node_1111111111111111111111111111111111111111111111111111hifc8npp"
        }"#;

        let deserialized: NodeIdDto = serde_json::from_str(json_str).unwrap();

        let node_id_dto = NodeIdDto {
            private: RawKey::zero(),
            public: PublicKey::zero(),
            as_account: Account::zero(),
            node_id: Account::zero(),
        };

        assert_eq!(deserialized, node_id_dto);
    }
}
