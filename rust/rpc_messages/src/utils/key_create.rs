use crate::RpcCommand;
use rsnano_core::{Account, PublicKey, RawKey};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn key_create() -> Self {
        Self::KeyCreate
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KeyPairDto {
    pub private: RawKey,
    pub public: PublicKey,
    pub account: Account,
}

impl KeyPairDto {
    pub fn new(private: RawKey, public: PublicKey, account: Account) -> Self {
        Self {
            private,
            public,
            account,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{KeyPairDto, RpcCommand};
    use rsnano_core::{Account, PublicKey, RawKey};
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_key_create_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::KeyCreate).unwrap(),
            r#"{
  "action": "key_create"
}"#
        )
    }

    #[test]
    fn deserialize_key_create_command() {
        let cmd = RpcCommand::key_create();
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn serialize_keypair_dto() {
        let keypair = KeyPairDto::new(RawKey::zero(), PublicKey::zero(), Account::zero());

        let serialized = serde_json::to_string_pretty(&keypair).unwrap();

        assert_eq!(
            serialized,
            r#"{
  "private": "0000000000000000000000000000000000000000000000000000000000000000",
  "public": "0000000000000000000000000000000000000000000000000000000000000000",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp"
}"#
        );
    }

    #[test]
    fn deserialize_keypair_dto() {
        let json_str = r#"{"private":"0000000000000000000000000000000000000000000000000000000000000000",
            "public":"0000000000000000000000000000000000000000000000000000000000000000",
            "account":"nano_1111111111111111111111111111111111111111111111111111hifc8npp"}"#;

        let deserialized: KeyPairDto = serde_json::from_str(json_str).unwrap();

        let expected = KeyPairDto::new(RawKey::zero(), PublicKey::zero(), Account::zero());

        assert_eq!(deserialized, expected);
    }
}
