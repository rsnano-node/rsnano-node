use crate::RpcCommand;
use rsnano_core::RawKey;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn key_expand(key: RawKey) -> Self {
        Self::KeyExpand(KeyExpandArgs::new(key))
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyExpandArgs {
    pub key: RawKey,
}

impl KeyExpandArgs {
    pub fn new(key: RawKey) -> Self {
        Self { key }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyExpandArgs, RpcCommand};
    use rsnano_core::RawKey;
    use serde_json::{from_str, to_string_pretty};

    #[test]
    fn serialize_key_expand_args() {
        let args = KeyExpandArgs {
            key: RawKey::zero(),
        };

        let serialized = serde_json::to_string(&args).unwrap();

        let expected =
            r#"{"key":"0000000000000000000000000000000000000000000000000000000000000000"}"#;

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize_key_expand_args() {
        let json_str =
            r#"{"key": "0000000000000000000000000000000000000000000000000000000000000000"}"#;
        let deserialized: KeyExpandArgs = from_str(json_str).unwrap();
        assert_eq!(
            deserialized,
            KeyExpandArgs {
                key: RawKey::zero(),
            }
        );
    }

    #[test]
    fn serialize_deterministic_key_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::KeyExpand(KeyExpandArgs::new(RawKey::zero(),))).unwrap(),
            r#"{
  "action": "key_expand",
  "key": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_deterministic_key_command() {
        let cmd = RpcCommand::KeyExpand(KeyExpandArgs::new(RawKey::zero()));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
