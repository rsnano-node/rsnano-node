use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CountRpcMessage {
    pub count: u64,
}

impl CountRpcMessage {
    pub fn new(count: u64) -> Self {
        Self { count }
    }
}
