use crate::RpcCommand;
use rsnano_core::RawKey;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn deterministic_key(seed: RawKey, index: u32) -> Self {
        Self::DeterministicKey(DeterministicKeyArgs::new(seed, index))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DeterministicKeyArgs {
    pub seed: RawKey,
    pub index: u32,
}

impl DeterministicKeyArgs {
    pub fn new(seed: RawKey, index: u32) -> Self {
        Self { seed, index }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeterministicKeyArgs, RpcCommand};
    use rsnano_core::RawKey;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_deterministic_key_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::DeterministicKey(DeterministicKeyArgs::new(
                RawKey::zero(),
                0
            )))
            .unwrap(),
            r#"{
  "action": "deterministic_key",
  "seed": "0000000000000000000000000000000000000000000000000000000000000000",
  "index": 0
}"#
        )
    }

    #[test]
    fn deserialize_deterministic_key_command() {
        let cmd = RpcCommand::DeterministicKey(DeterministicKeyArgs::new(RawKey::zero(), 0));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
