use crate::RpcCommand;
use rsnano_core::{Account, Amount, BlockHash, BlockSubType, Signature, WorkNonce};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn account_history(account_history_args: AccountHistoryArgs) -> Self {
        Self::AccountHistory(account_history_args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountHistoryArgs {
    pub account: Account,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<BlockHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_filter: Option<Vec<Account>>,
}

impl AccountHistoryArgs {
    pub fn new(account: Account, count: u64) -> AccountHistoryArgs {
        AccountHistoryArgs {
            account,
            count,
            raw: None,
            head: None,
            offset: None,
            reverse: None,
            account_filter: None,
        }
    }

    pub fn builder(account: Account, count: u64) -> AccountHistoryArgsBuilder {
        AccountHistoryArgsBuilder::new(account, count)
    }
}

pub struct AccountHistoryArgsBuilder {
    args: AccountHistoryArgs,
}

impl AccountHistoryArgsBuilder {
    fn new(account: Account, count: u64) -> Self {
        Self {
            args: AccountHistoryArgs {
                account,
                count,
                raw: None,
                head: None,
                offset: None,
                reverse: None,
                account_filter: None,
            },
        }
    }

    pub fn raw(mut self) -> Self {
        self.args.raw = Some(true);
        self
    }

    pub fn head(mut self, head: BlockHash) -> Self {
        self.args.head = Some(head);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.args.offset = Some(offset);
        self
    }

    pub fn reverse(mut self) -> Self {
        self.args.reverse = Some(true);
        self
    }

    pub fn account_filter(mut self, account_filter: Vec<Account>) -> Self {
        self.args.account_filter = Some(account_filter);
        self
    }

    pub fn build(self) -> AccountHistoryArgs {
        self.args
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountHistoryDto {
    pub account: Account,
    pub history: Vec<HistoryEntry>,
    pub previous: Option<BlockHash>,
    pub next: Option<BlockHash>,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(rename = "type")]
    pub block_type: BlockSubType,
    pub account: Account,
    pub amount: Amount,
    pub local_timestamp: u64,
    pub height: u64,
    pub hash: BlockHash,
    pub confirmed: bool,
    pub work: Option<WorkNonce>,
    pub signature: Option<Signature>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::to_string_pretty;

    #[test]
    fn serialize_account_history_command() {
        let account_history_args = AccountHistoryArgsBuilder::new(Account::zero(), 10)
            .raw()
            .head(BlockHash::zero())
            .offset(5)
            .reverse()
            .account_filter(vec![Account::from(123)])
            .build();

        assert_eq!(
            to_string_pretty(&RpcCommand::account_history(account_history_args)).unwrap(),
            r#"{
  "action": "account_history",
  "account": "nano_1111111111111111111111111111111111111111111111111111hifc8npp",
  "count": 10,
  "raw": true,
  "head": "0000000000000000000000000000000000000000000000000000000000000000",
  "offset": 5,
  "reverse": true,
  "account_filter": [
    "nano_111111111111111111111111111111111111111111111111115uwdgas549"
  ]
}"#
        )
    }

    #[test]
    fn deserialize_account_history_command() {
        let json = r#"{
            "action": "account_history",
            "account": "nano_111111111111111111111111111111111111111111111111115uwdgas549",
            "count": 5,
            "raw": true,
            "head": "0000000000000000000000000000000000000000000000000000000000000000",
            "offset": 10,
            "reverse": false,
            "account_filter": ["nano_1111111111111111111111111111111111111111111111111111hifc8npp"]
        }"#;

        let deserialized: RpcCommand = serde_json::from_str(json).unwrap();

        if let RpcCommand::AccountHistory(args) = deserialized {
            assert_eq!(args.account, Account::from(123));
            assert_eq!(args.count, 5);
            assert_eq!(args.raw, Some(true));
            assert_eq!(args.head, Some(BlockHash::zero()));
            assert_eq!(args.offset, Some(10));
            assert_eq!(args.reverse, Some(false));
            assert_eq!(args.account_filter, Some(vec![Account::zero()]));
        } else {
            panic!("Deserialized to wrong RpcCommand variant");
        }
    }
}
