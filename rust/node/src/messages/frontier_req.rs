use super::MessageHeaderExtender;
use anyhow::Result;
use bitvec::prelude::BitArray;
use rsnano_core::{
    utils::{Deserialize, FixedSizeSerialize, Serialize, Stream},
    Account,
};
use std::{fmt::Display, mem::size_of};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontierReq {
    pub start: Account,
    pub age: u32,
    pub count: u32,
    pub only_confirmed: bool,
}

impl FrontierReq {
    pub fn create_test_instance() -> Self {
        Self {
            start: 1.into(),
            age: 2,
            count: 3,
            only_confirmed: false,
        }
    }

    pub fn serialized_size() -> usize {
        Account::serialized_size()
        + size_of::<u32>() // age
        + size_of::<u32>() //count
    }

    pub const ONLY_CONFIRMED: usize = 1;

    pub fn deserialize(stream: &mut impl Stream, extensions: BitArray<u16>) -> Result<Self> {
        let start = Account::deserialize(stream)?;
        let mut buffer = [0u8; 4];
        stream.read_bytes(&mut buffer, 4)?;
        let age = u32::from_le_bytes(buffer);
        stream.read_bytes(&mut buffer, 4)?;
        let count = u32::from_le_bytes(buffer);
        let only_confirmed = extensions[FrontierReq::ONLY_CONFIRMED];

        Ok(FrontierReq {
            start,
            age,
            count,
            only_confirmed,
        })
    }
}

impl Serialize for FrontierReq {
    fn serialize(&self, stream: &mut dyn Stream) -> Result<()> {
        self.start.serialize(stream)?;
        stream.write_bytes(&self.age.to_le_bytes())?;
        stream.write_bytes(&self.count.to_le_bytes())
    }
}

impl MessageHeaderExtender for FrontierReq {
    fn header_extensions(&self, _payload_len: u16) -> BitArray<u16> {
        let mut extensions = BitArray::default();
        extensions.set(Self::ONLY_CONFIRMED, self.only_confirmed);
        extensions
    }
}

impl Display for FrontierReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\nstart={} maxage={} count={}",
            self.start, self.age, self.count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{assert_deserializable, Message};

    #[test]
    fn serialize() {
        let request = Message::FrontierReq(FrontierReq::create_test_instance());
        assert_deserializable(&request);
    }
}
