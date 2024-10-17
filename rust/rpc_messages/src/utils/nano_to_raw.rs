use crate::{common::AmountRpcMessage, RpcCommand};
use rsnano_core::Amount;

impl RpcCommand {
    pub fn nano_to_raw(amount: Amount) -> Self {
        Self::NanoToRaw(AmountRpcMessage::new(amount))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Amount;
    use serde_json::{from_str, to_string_pretty};

    #[test]
    fn serialize_nano_to_raw_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::nano_to_raw(Amount::nano(1))).unwrap(),
            r#"{
  "action": "nano_to_raw",
  "amount": "1000000000000000000000000000000"
}"#
        );
    }

    #[test]
    fn deserialize_nano_to_raw_command() {
        let cmd = RpcCommand::nano_to_raw(Amount::nano(1));
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);
    }
}
