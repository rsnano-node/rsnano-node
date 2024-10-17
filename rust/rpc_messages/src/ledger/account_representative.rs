use crate::{common::AccountRpcMessage, RpcCommand};
use rsnano_core::Account;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn account_representative(account: Account) -> Self {
        Self::AccountRepresentative(AccountRpcMessage::new(account))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountRepresentativeArgs {
    pub account: Account,
}

impl AccountRepresentativeArgs {
    pub fn new(account: Account) -> Self {
        Self { account }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountRepresentativeDto {
    pub representative: Account,
}

impl AccountRepresentativeDto {
    pub fn new(representative: Account) -> Self {
        Self { representative }
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use rsnano_core::Account;
    use serde_json::{from_str, to_string_pretty};

    #[test]
    fn serialize_account_representative_command() {
        assert_eq!(
            serde_json::to_string_pretty(&RpcCommand::account_representative(Account::from(123)))
                .unwrap(),
            r#"{
  "action": "account_representative",
  "account": "nano_111111111111111111111111111111111111111111111111115uwdgas549"
}"#
        )
    }

    #[test]
    fn derialize_account_representative_command() {
        let account = Account::from(123);
        let cmd = RpcCommand::account_representative(account);
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }
}
