use crate::{common::AmountRpcMessage, RpcCommand};
use rsnano_core::Amount;

impl RpcCommand {
    pub fn raw_to_nano(amount: Amount) -> Self {
        Self::RawToNano(AmountRpcMessage::new(amount))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Amount;
    use serde_json::{from_str, to_string_pretty};

    #[test]
    fn serialize_raw_to_nano_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::raw_to_nano(Amount::nano(1))).unwrap(),
            r#"{
  "action": "raw_to_nano",
  "amount": "1000000000000000000000000000000"
}"#
        );
    }

    #[test]
    fn deserialize_raw_to_nano_command() {
        let cmd = RpcCommand::raw_to_nano(Amount::nano(1));
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);
    }
}
