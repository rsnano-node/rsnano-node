use rsnano_core::Amount;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AmountRpcMessage {
    pub amount: Amount,
}

impl AmountRpcMessage {
    pub fn new(amount: Amount) -> Self {
        Self { amount }
    }
}
