use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DestroyedDto {
    pub destroyed: bool,
}

impl DestroyedDto {
    pub fn new(destroyed: bool) -> Self {
        Self { destroyed }
    }
}
