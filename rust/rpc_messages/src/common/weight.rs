use rsnano_core::Amount;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WeightDto {
    pub weight: Amount,
}

impl WeightDto {
    pub fn new(weight: Amount) -> Self {
        Self { weight }
    }
}
