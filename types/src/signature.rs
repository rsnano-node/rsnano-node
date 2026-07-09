use serde::de::{Unexpected, Visitor};
use std::{
    fmt::{Debug, Write},
    io::Read,
};
use thiserror::Error;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signature {
    bytes: [u8; 64],
}

impl Signature {
    pub const SERIALIZED_SIZE: usize = 64;

    pub fn new() -> Self {
        Self { bytes: [0u8; 64] }
    }

    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self { bytes }
    }

    pub fn deserialize<T>(reader: &mut T) -> std::io::Result<Signature>
    where
        T: Read,
    {
        let mut result = Signature { bytes: [0; 64] };
        reader.read_exact(&mut result.bytes)?;
        Ok(result)
    }

    pub fn as_bytes(&'_ self) -> &'_ [u8; 64] {
        &self.bytes
    }

    pub fn encode_hex(&self) -> String {
        let mut result = String::with_capacity(128);
        for byte in self.bytes {
            write!(&mut result, "{:02X}", byte).unwrap();
        }
        result
    }

    pub fn decode_hex(s: impl AsRef<str>) -> Option<Self> {
        let mut bytes = [0u8; 64];
        hex::decode_to_slice(s.as_ref(), &mut bytes).ok()?;
        Some(Signature::from_bytes(bytes))
    }

    pub fn serialize<T>(&self, writer: &mut T) -> std::io::Result<()>
    where
        T: std::io::Write,
    {
        writer.write_all(&self.bytes)
    }
}

impl Default for Signature {
    fn default() -> Self {
        Self { bytes: [0; 64] }
    }
}

impl Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.bytes {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl serde::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = deserializer.deserialize_str(SignatureVisitor {})?;
        Ok(value)
    }
}

pub(crate) struct SignatureVisitor {}

impl<'de> Visitor<'de> for SignatureVisitor {
    type Value = Signature;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a hex string containing 64 bytes")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let signature = Signature::decode_hex(v).ok_or_else(|| {
            serde::de::Error::invalid_value(Unexpected::Str(v), &"a hex string containing 64 bytes")
        })?;
        Ok(signature)
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub struct SignatureError {}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid signature")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_format() {
        let signature = Signature::from_bytes([42; 64]);
        let result = format!("{:?}", signature);
        assert_eq!(
            result,
            "2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A"
        );
    }
}
