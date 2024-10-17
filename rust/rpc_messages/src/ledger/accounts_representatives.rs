use crate::{common::AccountsRpcMessage, RpcCommand};
use rsnano_core::Account;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

impl RpcCommand {
    pub fn accounts_representatives(accounts: Vec<Account>) -> Self {
        Self::AccountsRepresentatives(AccountsRpcMessage::new(accounts))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountsRepresentativesDto {
    pub representatives: HashMap<Account, Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<HashMap<Account, String>>,
}

impl AccountsRepresentativesDto {
    pub fn new(representatives: HashMap<Account, Account>) -> Self {
        Self {
            representatives,
            errors: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::Account;
    use serde_json::{from_str, to_string_pretty};

    #[test]
    fn serialize_accounts_representatives_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::accounts_representatives(vec![Account::from(
                123
            )]))
            .unwrap(),
            r#"{
  "action": "accounts_representatives",
  "accounts": [
    "nano_111111111111111111111111111111111111111111111111115uwdgas549"
  ]
}"#
        )
    }

    #[test]
    fn deserialize_accounts_representatives_command() {
        let account = Account::from(123);
        let cmd = RpcCommand::accounts_representatives(vec![account]);
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized)
    }

    #[test]
    fn serialize_accounts_representatives_dto_without_errors() {
        let mut representatives = HashMap::new();
        representatives.insert(Account::from(123), Account::from(456));
        let dto = AccountsRepresentativesDto::new(representatives);

        assert_eq!(
            to_string_pretty(&dto).unwrap(),
            r#"{
  "representatives": {
    "nano_111111111111111111111111111111111111111111111111115uwdgas549": "nano_11111111111111111111111111111111111111111111111111gahteczqci"
  }
}"#
        );
    }

    #[test]
    fn deserialize_accounts_representatives_dto_without_errors() {
        let json = r#"{
  "representatives": {
    "nano_111111111111111111111111111111111111111111111111115uwdgas549": "nano_11111111111111111111111111111111111111111111111111gahteczqci"
  }
}"#;
        let dto: AccountsRepresentativesDto = from_str(json).unwrap();

        assert_eq!(dto.representatives.len(), 1);
        assert_eq!(dto.errors, None);
        assert_eq!(
            dto.representatives.get(&Account::from(123)),
            Some(&Account::from(456))
        );
    }

    #[test]
    fn serialize_accounts_representatives_dto_with_errors() {
        let mut representatives = HashMap::new();
        representatives.insert(Account::from(123), Account::from(456));
        let mut errors = HashMap::new();
        errors.insert(Account::from(789), "Invalid account".to_string());

        let mut dto = AccountsRepresentativesDto::new(representatives);
        dto.errors = Some(errors);

        assert_eq!(
            to_string_pretty(&dto).unwrap(),
            r#"{
  "representatives": {
    "nano_111111111111111111111111111111111111111111111111115uwdgas549": "nano_11111111111111111111111111111111111111111111111111gahteczqci"
  },
  "errors": {
    "nano_11111111111111111111111111111111111111111111111111ros3kc7wyy": "Invalid account"
  }
}"#
        );
    }

    #[test]
    fn deserialize_accounts_representatives_dto_with_errors() {
        let json = r#"{
  "representatives": {
    "nano_111111111111111111111111111111111111111111111111115uwdgas549": "nano_11111111111111111111111111111111111111111111111111gahteczqci"
  },
  "errors": {
    "nano_11111111111111111111111111111111111111111111111111ros3kc7wyy": "Invalid account"
  }
}"#;
        let dto: AccountsRepresentativesDto = from_str(json).unwrap();

        assert_eq!(dto.representatives.len(), 1);
        assert!(dto.errors.is_some());
        assert_eq!(dto.errors.as_ref().unwrap().len(), 1);
        assert_eq!(
            dto.representatives.get(&Account::from(123)),
            Some(&Account::from(456))
        );
        assert_eq!(
            dto.errors.as_ref().unwrap().get(&Account::from(789)),
            Some(&"Invalid account".to_string())
        );
    }
}
