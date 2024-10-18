use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ValidDto {
    pub valid: bool,
}

impl ValidDto {
    pub fn new(valid: bool) -> Self {
        Self { valid }
    }
}
