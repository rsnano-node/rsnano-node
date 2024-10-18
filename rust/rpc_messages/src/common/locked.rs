use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LockedDto {
    pub locked: bool,
}

impl LockedDto {
    pub fn new(locked: bool) -> Self {
        Self { locked }
    }
}
