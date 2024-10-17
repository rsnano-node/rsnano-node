use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StartedDto {
    pub started: bool,
}

impl StartedDto {
    pub fn new(started: bool) -> Self {
        Self { started }
    }
}
