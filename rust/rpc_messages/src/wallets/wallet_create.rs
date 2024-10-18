use crate::RpcCommand;
use rsnano_core::{RawKey, WalletId};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn wallet_create(seed: Option<RawKey>) -> Self {
        Self::WalletCreate(WalletCreateArgs::new(seed))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletCreateArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<RawKey>,
}

impl WalletCreateArgs {
    pub fn new(seed: Option<RawKey>) -> Self {
        WalletCreateArgs { seed }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletCreateDto {
    pub wallet: WalletId,
}

impl WalletCreateDto {
    pub fn new(wallet: WalletId) -> Self {
        Self { wallet }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::RawKey;
    use serde_json::{from_str, to_string, to_string_pretty};

    #[test]
    fn serialize_wallet_create_command_seed_none() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_create(None)).unwrap(),
            r#"{
  "action": "wallet_create"
}"#
        )
    }

    #[test]
    fn serialize_wallet_create_command_seed_some() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_create(Some(RawKey::zero()))).unwrap(),
            r#"{
  "action": "wallet_create",
  "seed": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn deserialize_wallet_create_command_seed_none() {
        let cmd = RpcCommand::wallet_create(None);
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn deserialize_wallet_create_command_seed_some() {
        let cmd = RpcCommand::wallet_create(Some(RawKey::zero()));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn serialize_wallet_rpc_message() {
        let wallet_rpc_message = WalletCreateDto::new(WalletId::zero());

        let serialized = to_string(&wallet_rpc_message).unwrap();

        let expected_json = serde_json::json!({
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        });

        let actual_json: serde_json::Value = from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn deserialize_wallet_rpc_message() {
        let json_str = r#"{
            "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
        }"#;

        let deserialized: WalletCreateDto = from_str(json_str).unwrap();

        let expected = WalletCreateDto::new(WalletId::zero());

        assert_eq!(deserialized, expected);
    }
}
