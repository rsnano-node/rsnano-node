use rsnano_core::Amount;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountBalanceDto {
    pub balance: Amount,
    pub pending: Amount,
    pub receivable: Amount,
}

impl AccountBalanceDto {
    pub fn new(balance: Amount, pending: Amount, receivable: Amount) -> Self {
        Self {
            balance,
            pending,
            receivable,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::common::AccountBalanceDto;
    use rsnano_core::Amount;

    #[test]
    fn serialize_account_balance_dto() {
        let account_balance = AccountBalanceDto {
            balance: Amount::raw(1000),
            pending: Amount::raw(200),
            receivable: Amount::raw(300),
        };

        let serialized = serde_json::to_string(&account_balance).unwrap();

        assert_eq!(
            serialized,
            r#"{"balance":"1000","pending":"200","receivable":"300"}"#
        );
    }

    #[test]
    fn deserialize_account_balance_dto() {
        let json_str = r#"{"balance":"1000","pending":"200","receivable":"300"}"#;

        let deserialized: AccountBalanceDto = serde_json::from_str(json_str).unwrap();

        let expected = AccountBalanceDto {
            balance: Amount::raw(1000),
            pending: Amount::raw(200),
            receivable: Amount::raw(300),
        };

        assert_eq!(deserialized, expected);
    }
}
