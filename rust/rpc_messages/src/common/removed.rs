use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RemovedDto {
    pub removed: bool,
}

impl RemovedDto {
    pub fn new(removed: bool) -> Self {
        Self { removed }
    }
}
