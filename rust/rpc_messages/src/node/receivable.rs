use crate::RpcCommand;
use rsnano_core::{Account, Amount};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn receivable(args: ReceivableArgs) -> Self {
        Self::Receivable(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ReceivableArgs {
    pub account: Account,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sorting: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_only_confirmed: Option<bool>,
}

impl ReceivableArgs {
    pub fn new(account: Account, count: u64) -> Self {
        Self {
            account,
            count,
            threshold: None,
            source: None,
            min_version: None,
            sorting: None,
            include_only_confirmed: None,
        }
    }

    pub fn builder(account: Account, count: u64) -> ReceivableArgsBuilder {
        ReceivableArgsBuilder {
            args: ReceivableArgs::new(account, count),
        }
    }
}

pub struct ReceivableArgsBuilder {
    args: ReceivableArgs,
}

impl ReceivableArgsBuilder {
    pub fn threshold(mut self, threshold: Amount) -> Self {
        self.args.threshold = Some(threshold);
        self
    }

    pub fn include_unconfirmed_blocks(mut self) -> Self {
        self.args.include_only_confirmed = Some(false);
        self
    }

    pub fn min_version(mut self) -> Self {
        self.args.min_version = Some(true);
        self
    }

    pub fn sorting(mut self) -> Self {
        self.args.sorting = Some(true);
        self
    }

    pub fn source(mut self) -> Self {
        self.args.source = Some(true);
        self
    }

    pub fn build(self) -> ReceivableArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use super::{ReceivableArgs, RpcCommand};
    use rsnano_core::Account;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_receivable_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::receivable(ReceivableArgs::new(
                Account::zero(),
                1,
            )))
            .unwrap(),
            r#"{
  "action": "receivable",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp",
  "count": 1
}"#
        )
    }

    #[test]
    fn deserialize_receivable_command() {
        let cmd = RpcCommand::receivable(ReceivableArgs::new(Account::zero(), 1));
        let serialized = serde_json::to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
