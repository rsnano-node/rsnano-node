use crate::RpcCommand;
use rsnano_core::{Account, Amount};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn ledger(ledger_args: LedgerArgs) -> Self {
        Self::Ledger(ledger_args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct LedgerArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receivable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_since: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sorting: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<Amount>,
}

impl LedgerArgs {
    pub fn builder() -> LedgerArgsBuilder {
        LedgerArgsBuilder {
            args: LedgerArgs::default(),
        }
    }
}

pub struct LedgerArgsBuilder {
    args: LedgerArgs,
}

impl LedgerArgsBuilder {
    pub fn from_account(mut self, account: Account) -> Self {
        self.args.account = Some(account);
        self
    }

    pub fn count(mut self, count: u64) -> Self {
        self.args.count = Some(count);
        self
    }

    pub fn include_representative(mut self) -> Self {
        self.args.representative = Some(true);
        self
    }

    pub fn include_weight(mut self) -> Self {
        self.args.weight = Some(true);
        self
    }

    pub fn include_receivables(mut self) -> Self {
        self.args.receivable = Some(true);
        self
    }

    pub fn modified_since(mut self, modified_since: u64) -> Self {
        self.args.modified_since = Some(modified_since);
        self
    }

    pub fn sorted(mut self) -> Self {
        self.args.sorting = Some(true);
        self
    }

    pub fn with_minimum_balance(mut self, threshold: Amount) -> Self {
        self.args.threshold = Some(threshold);
        self
    }

    pub fn build(self) -> LedgerArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use crate::{LedgerArgs, RpcCommand};
    use rsnano_core::{Account, Amount};
    use serde_json::json;

    #[test]
    fn test_ledger_rpc_command_serialization() {
        let account = Account::decode_account(
            "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
        )
        .unwrap();
        let ledger_args = LedgerArgs::builder()
            .from_account(account)
            .count(1000)
            .include_representative()
            .include_weight()
            .include_receivables()
            .modified_since(1234567890)
            .sorted()
            .with_minimum_balance(Amount::raw(1000000000000000000000000000000u128))
            .build();

        let rpc_command = RpcCommand::Ledger(ledger_args);

        let serialized = serde_json::to_value(&rpc_command).unwrap();

        let expected = json!({
            "action": "ledger",
            "account": "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
            "count": 1000,
            "representative": true,
            "weight": true,
            "receivable": true,
            "modified_since": 1234567890,
            "sorting": true,
            "threshold": "1000000000000000000000000000000"
        });

        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_ledger_rpc_command_deserialization() {
        let json_str = r#"{
            "action": "ledger",
            "account": "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est",
            "count": 1000,
            "representative": true,
            "weight": true,
            "pending": true,
            "receivable": true,
            "modified_since": 1234567890,
            "sorting": true,
            "threshold": "1000000000000000000000000000000"
        }"#;

        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();

        match deserialized {
            RpcCommand::Ledger(args) => {
                assert_eq!(
                    args.account,
                    Some(
                        Account::decode_account(
                            "nano_1ipx847tk8o46pwxt5qjdbncjqcbwcc1rrmqnkztrfjy5k7z4imsrata9est"
                        )
                        .unwrap()
                    )
                );
                assert_eq!(args.count, Some(1000));
                assert_eq!(args.representative, Some(true));
                assert_eq!(args.weight, Some(true));
                assert_eq!(args.pending, Some(true));
                assert_eq!(args.receivable, Some(true));
                assert_eq!(args.modified_since, Some(1234567890));
                assert_eq!(args.sorting, Some(true));
                assert_eq!(
                    args.threshold,
                    Some(Amount::raw(1000000000000000000000000000000u128))
                );
            }
            _ => panic!("Deserialized to wrong variant"),
        }
    }
}