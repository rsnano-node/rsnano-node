use rsnano_core::Account;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountRpcMessage {
    pub account: Account,
}

impl AccountRpcMessage {
    pub fn new(account: Account) -> Self {
        Self { account }
    }
}
