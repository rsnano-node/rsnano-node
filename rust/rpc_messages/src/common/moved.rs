use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MovedDto {
    pub moved: bool,
}

impl MovedDto {
    pub fn new(moved: bool) -> Self {
        Self { moved }
    }
}
