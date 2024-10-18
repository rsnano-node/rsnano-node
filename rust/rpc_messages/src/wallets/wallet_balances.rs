use crate::RpcCommand;
use rsnano_core::{Amount, WalletId};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn wallet_balances(args: WalletBalancesArgs) -> Self {
        Self::WalletBalances(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletBalancesArgs {
    pub wallet: WalletId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<Amount>,
}

impl WalletBalancesArgs {
    pub fn builder(wallet: WalletId) -> WalletBalancesArgsBuilder {
        WalletBalancesArgsBuilder::new(wallet)
    }
}

impl From<WalletId> for WalletBalancesArgs {
    fn from(wallet: WalletId) -> Self {
        Self {
            wallet,
            threshold: None,
        }
    }
}

pub struct WalletBalancesArgsBuilder {
    args: WalletBalancesArgs,
}

impl WalletBalancesArgsBuilder {
    fn new(wallet: WalletId) -> Self {
        Self {
            args: WalletBalancesArgs {
                wallet,
                threshold: None,
            },
        }
    }

    pub fn with_minimum_balance(mut self, threshold: Amount) -> Self {
        self.args.threshold = Some(threshold);
        self
    }

    pub fn build(self) -> WalletBalancesArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use crate::{wallets::WalletBalancesArgs, RpcCommand};
    use rsnano_core::{Amount, WalletId};
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_wallet_balances_command_threshold_none() {
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_balances(WalletId::zero().into())).unwrap(),
            r#"{
  "action": "wallet_balances",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000000"
}"#
        )
    }

    #[test]
    fn serialize_wallet_balances_command_threshold_some() {
        let args = WalletBalancesArgs::builder(1.into())
            .with_minimum_balance(Amount::zero())
            .build();
        assert_eq!(
            to_string_pretty(&RpcCommand::wallet_balances(args)).unwrap(),
            r#"{
  "action": "wallet_balances",
  "wallet": "0000000000000000000000000000000000000000000000000000000000000001",
  "threshold": "0"
}"#
        )
    }

    #[test]
    fn deserialize_wallet_balances_command_threshold_none() {
        let cmd = RpcCommand::wallet_balances(WalletId::zero().into());
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn deserialize_wallet_balances_command_threshold_some() {
        let args = WalletBalancesArgs::builder(1.into())
            .with_minimum_balance(Amount::zero())
            .build();
        let cmd = RpcCommand::wallet_balances(args);
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn wallet_balances_args_builder() {
        let wallet = 1.into();
        let args = WalletBalancesArgs::builder(wallet)
            .with_minimum_balance(Amount::raw(1000))
            .build();

        assert_eq!(args.wallet, wallet);
        assert_eq!(args.threshold, Some(Amount::raw(1000)));
    }

    #[test]
    fn wallet_balances_args_from_wallet_id() {
        let wallet: WalletId = 1.into();
        let args: WalletBalancesArgs = wallet.into();

        assert_eq!(args.wallet, wallet);
        assert_eq!(args.threshold, None);
    }
}
