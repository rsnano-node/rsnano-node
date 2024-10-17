use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ExistsDto {
    pub exists: bool,
}

impl ExistsDto {
    pub fn new(exists: bool) -> Self {
        Self { exists }
    }
}
