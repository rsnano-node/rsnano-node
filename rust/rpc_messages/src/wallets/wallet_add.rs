use crate::RpcCommand;
use rsnano_core::{RawKey, WalletId};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn wallet_add(args: WalletAddArgs) -> Self {
        Self::WalletAdd(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletAddArgs {
    pub wallet: WalletId,
    pub key: RawKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<bool>,
}

impl WalletAddArgs {
    pub fn new(wallet: WalletId, key: RawKey) -> WalletAddArgs {
        WalletAddArgs {
            wallet,
            key,
            work: None,
        }
    }

    pub fn builder(wallet: WalletId, key: RawKey) -> WalletAddArgsBuilder {
        WalletAddArgsBuilder::new(wallet, key)
    }
}

pub struct WalletAddArgsBuilder {
    args: WalletAddArgs,
}

impl WalletAddArgsBuilder {
    pub fn new(wallet: WalletId, key: RawKey) -> Self {
        Self {
            args: WalletAddArgs {
                wallet,
                key,
                work: None,
            },
        }
    }

    pub fn without_precomputed_work(mut self) -> Self {
        self.args.work = Some(false);
        self
    }

    pub fn build(self) -> WalletAddArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use crate::{wallets::WalletAddArgs, RpcCommand};
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_wallet_add_command_work_none() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_add(WalletAddArgs::new(
                1.into(),
                2.into()
            )))
            .unwrap(),
            r#"{
  "action": "wallet_add",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001",
  "key": "0000000000000000000000000000000000000000000000000000000000000002"
}"#
        )
    }

    #[test]
    fn serialize_wallet_add_command_work_some() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_add(
                WalletAddArgs::builder(1.into(), 2.into())
                    .without_precomputed_work()
                    .build()
            ))
            .unwrap(),
            r#"{
  "action": "wallet_add",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001",
  "key": "0000000000000000000000000000000000000000000000000000000000000002",
  "work": false
}"#
        )
    }

    #[test]
    fn deserialize_wallet_add_command_work_none() {
        let cmd = RpcCommand::wallet_add(WalletAddArgs::new(1.into(), 2.into()));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn deserialize_wallet_add_command_work_some() {
        let cmd = RpcCommand::wallet_add(WalletAddArgs::new(1.into(), 2.into()));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
