use super::MessageVariant;
use anyhow::Result;
use bitvec::prelude::BitArray;
use rsnano_core::utils::{
    BufferWriter, Deserialize, FixedSizeSerialize, MemoryStream, Serialize, Stream, StreamExt,
};
use rsnano_core::{
    sign_message, to_hex_string, validate_message, Account, BlockHash, KeyPair, PublicKey,
    Signature,
};
use serde::ser::SerializeStruct;
use serde::{Deserializer, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::fmt::Display;
use std::mem::size_of;
use std::time::{Duration, SystemTime};

#[repr(u8)]
#[derive(FromPrimitive, Copy, Clone, PartialEq, Eq)]
pub enum TelemetryMaker {
    NfNode = 0,
    NfPrunedNode = 1,
    NanoNodeLight = 2,
    RsNano = 3,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TelemetryData {
    pub signature: Signature,
    pub node_id: PublicKey,
    pub block_count: u64,
    pub cemented_count: u64,
    pub unchecked_count: u64,
    pub account_count: u64,
    pub bandwidth_cap: u64,
    pub uptime: u64,
    pub peer_count: u32,
    pub protocol_version: u8,
    pub genesis_block: BlockHash,
    pub major_version: u8,
    pub minor_version: u8,
    pub patch_version: u8,
    pub pre_release_version: u8,
    pub maker: u8, // Where this telemetry information originated
    #[serde(
        serialize_with = "serialize_timestamp",
        deserialize_with = "deserialize_timestamp"
    )]
    pub timestamp: SystemTime,
    pub active_difficulty: u64,
    pub unknown_data: Vec<u8>,
}

fn serialize_timestamp<S>(timestamp: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let duration = timestamp
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs();
    let nanos = duration.subsec_nanos();
    let mut state = serializer.serialize_struct("SystemTime", 2)?;
    state.serialize_field("secs", &seconds)?;
    state.serialize_field("nanos", &nanos)?;
    state.end()
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Helper {
        secs: u64,
        nanos: u32,
    }

    use serde::Deserialize;

    let Helper { secs, nanos } = Helper::deserialize(deserializer)?;
    Ok(SystemTime::UNIX_EPOCH + Duration::new(secs, nanos))
}

impl TelemetryData {
    pub const SIZE_MASK: u16 = 0x3ff;

    pub fn new() -> Self {
        Self {
            signature: Signature::new(),
            node_id: PublicKey::zero(),
            block_count: 0,
            cemented_count: 0,
            unchecked_count: 0,
            account_count: 0,
            bandwidth_cap: 0,
            uptime: 0,
            peer_count: 0,
            protocol_version: 0,
            genesis_block: BlockHash::zero(),
            major_version: 0,
            minor_version: 0,
            patch_version: 0,
            pre_release_version: 0,
            maker: TelemetryMaker::RsNano as u8,
            timestamp: SystemTime::UNIX_EPOCH,
            active_difficulty: 0,
            unknown_data: Vec::new(),
        }
    }

    pub fn new_test_instance() -> Self {
        let mut data = TelemetryData::new();
        data.node_id = PublicKey::from(42);
        data.major_version = 20;
        data.minor_version = 1;
        data.patch_version = 5;
        data.pre_release_version = 2;
        data.maker = TelemetryMaker::RsNano as u8;
        data.timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(100);
        data
    }

    /// Size does not include unknown_data
    pub fn serialized_size_of_known_data() -> usize {
        Signature::serialized_size()
        + Account::serialized_size() // node id
        + size_of::<u64>() //block_count
          + size_of::<u64>()// cemented_count 
          + size_of::<u64>() // unchecked_count 
          + size_of::<u64>() // account_count 
          + size_of::<u64>() // bandwidth_cap 
          + size_of::<u32>() // peer_count
          + size_of::<u8>() // protocol_version
          + size_of::<u64>() // uptime 
          + BlockHash::serialized_size()
          + size_of::<u8>() // major_version 
          + size_of::<u8>() // minor_version 
          + size_of::<u8>() // patch_version 
          + size_of::<u8>() // pre_release_version 
          + size_of::<u8>() // maker 
          + size_of::<u64>() // timestamp 
          + size_of::<u64>() //active_difficulty)
    }

    pub fn serialize_without_signature(&self, writer: &mut dyn BufferWriter) {
        // All values should be serialized in big endian
        self.node_id.serialize(writer);
        writer.write_u64_be_safe(self.block_count);
        writer.write_u64_be_safe(self.cemented_count);
        writer.write_u64_be_safe(self.unchecked_count);
        writer.write_u64_be_safe(self.account_count);
        writer.write_u64_be_safe(self.bandwidth_cap);
        writer.write_u32_be_safe(self.peer_count);
        writer.write_u8_safe(self.protocol_version);
        writer.write_u64_be_safe(self.uptime);
        self.genesis_block.serialize(writer);
        writer.write_u8_safe(self.major_version);
        writer.write_u8_safe(self.minor_version);
        writer.write_u8_safe(self.patch_version);
        writer.write_u8_safe(self.pre_release_version);
        writer.write_u8_safe(self.maker);
        writer.write_u64_be_safe(
            self.timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );
        writer.write_u64_be_safe(self.active_difficulty);
        writer.write_bytes_safe(&self.unknown_data);
    }

    pub fn deserialize(stream: &mut dyn Stream, payload_len: usize) -> anyhow::Result<Self> {
        let signature = Signature::deserialize(stream)?;
        let node_id = PublicKey::deserialize(stream)?;
        let block_count = stream.read_u64_be()?;
        let cemented_count = stream.read_u64_be()?;
        let unchecked_count = stream.read_u64_be()?;
        let account_count = stream.read_u64_be()?;
        let bandwidth_cap = stream.read_u64_be()?;
        let peer_count = stream.read_u32_be()?;
        let protocol_version = stream.read_u8()?;
        let uptime = stream.read_u64_be()?;
        let genesis_block = BlockHash::deserialize(stream)?;
        let major_version = stream.read_u8()?;
        let minor_version = stream.read_u8()?;
        let patch_version = stream.read_u8()?;
        let pre_release_version = stream.read_u8()?;
        let maker = stream.read_u8()?;
        let timestamp_ms = stream.read_u64_be()?;
        let active_difficulty = stream.read_u64_be()?;
        let mut unknown_data = Vec::new();
        if payload_len as usize > TelemetryData::serialized_size_of_known_data() {
            let unknown_len =
                (payload_len as usize) - TelemetryData::serialized_size_of_known_data();
            unknown_data.resize(unknown_len, 0);
            stream.read_bytes(&mut unknown_data, unknown_len)?;
        }

        let data = TelemetryData {
            signature,
            node_id,
            block_count,
            cemented_count,
            unchecked_count,
            account_count,
            bandwidth_cap,
            peer_count,
            protocol_version,
            uptime,
            genesis_block,
            major_version,
            minor_version,
            patch_version,
            pre_release_version,
            maker,
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_millis(timestamp_ms),
            active_difficulty,
            unknown_data,
        };

        Ok(data)
    }

    pub fn sign(&mut self, keys: &KeyPair) -> Result<()> {
        debug_assert!(keys.public_key() == self.node_id);
        let mut stream = MemoryStream::new();
        self.serialize_without_signature(&mut stream);
        self.signature = sign_message(&keys.private_key(), stream.as_bytes());
        Ok(())
    }

    pub fn validate_signature(&self) -> bool {
        let mut stream = MemoryStream::new();
        self.serialize_without_signature(&mut stream);
        validate_message(&self.node_id, stream.as_bytes(), &self.signature).is_ok()
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        let ignore_identification_metrics = true;
        let json_dto = TelemetryDataJsonDto {
            block_count: self.block_count.to_string(),
            cemented_count: self.cemented_count.to_string(),
            unchecked_count: self.unchecked_count.to_string(),
            account_count: self.account_count.to_string(),
            bandwidth_cap: self.bandwidth_cap.to_string(),
            peer_count: self.peer_count.to_string(),
            protocol_version: self.protocol_version.to_string(),
            uptime: self.uptime.to_string(),
            genesis_block: self.genesis_block.to_string(),
            major_version: self.major_version.to_string(),
            minor_version: self.minor_version.to_string(),
            patch_version: self.patch_version.to_string(),
            pre_release_version: self.pre_release_version.to_string(),
            maker: self.maker.to_string(),
            timestamp: self
                .timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
            active_difficulty: to_hex_string(self.active_difficulty),
            node_id: if !ignore_identification_metrics {
                Some(self.node_id.to_node_id())
            } else {
                None
            },
            signature: if !ignore_identification_metrics {
                Some(self.signature.encode_hex())
            } else {
                None
            },
        };

        serde_json::to_string_pretty(&json_dto)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryAck(pub Option<TelemetryData>);

impl serde::Serialize for TelemetryAck {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(data) = &self.0 {
            let mut state = serializer.serialize_struct("TelemetryAck", 1)?;
            state.serialize_field("data", data)?;
            state.end()
        } else {
            serializer.serialize_none()
        }
    }
}

impl TelemetryAck {
    pub fn new_test_instance() -> Self {
        Self(Some(TelemetryData::new_test_instance()))
    }

    pub fn serialized_size(extensions: BitArray<u16>) -> usize {
        (extensions.data & TelemetryData::SIZE_MASK) as usize
    }

    pub fn deserialize(stream: &mut dyn Stream, extensions: BitArray<u16>) -> Option<Self> {
        let payload_length = Self::serialized_size(extensions);
        if payload_length == 0 {
            return Some(Self(None));
        }

        let result = TelemetryData::deserialize(stream, payload_length).ok()?;

        Some(Self(Some(result)))
    }
}

impl Display for TelemetryAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "telemetry_ack")
    }
}

impl Serialize for TelemetryAck {
    fn serialize(&self, writer: &mut dyn BufferWriter) {
        if let Some(data) = &self.0 {
            data.signature.serialize(writer);
            data.serialize_without_signature(writer);
        }
    }
}

impl MessageVariant for TelemetryAck {
    fn header_extensions(&self, _payload_len: u16) -> BitArray<u16> {
        match &self.0 {
            Some(data) => BitArray::new(
                TelemetryData::serialized_size_of_known_data() as u16
                    + data.unknown_data.len() as u16,
            ),
            None => Default::default(),
        }
    }
}

#[derive(Serialize)]
struct TelemetryDataJsonDto {
    pub block_count: String,
    pub cemented_count: String,
    pub unchecked_count: String,
    pub account_count: String,
    pub bandwidth_cap: String,
    pub peer_count: String,
    pub protocol_version: String,
    pub uptime: String,
    pub genesis_block: String,
    pub major_version: String,
    pub minor_version: String,
    pub patch_version: String,
    pub pre_release_version: String,
    pub maker: String,
    pub timestamp: String,
    pub active_difficulty: String,
    // Keep these last for UI purposes:
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl Default for TelemetryData {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for TelemetryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        write!(f, "{}", self.to_json().map_err(|_| std::fmt::Error)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assert_deserializable, Message};
    use std::thread::sleep;

    #[test]
    fn serialized_size() {
        assert_eq!(TelemetryData::serialized_size_of_known_data(), 202);
    }

    // original test: telemetry.signatures
    #[test]
    fn sign_telemetry_data() -> Result<()> {
        let keys = KeyPair::new();
        let mut data = test_data(&keys);
        data.sign(&keys)?;
        assert_eq!(data.validate_signature(), true);

        let old_signature = data.signature.clone();
        // Check that the signature is different if changing a piece of data
        data.maker = 2;
        data.sign(&keys)?;
        assert_ne!(old_signature, data.signature);
        Ok(())
    }

    //original test: telemetry.unknown_data
    #[test]
    fn sign_with_unknown_data() -> Result<()> {
        let keys = KeyPair::new();
        let mut data = test_data(&keys);
        data.unknown_data = vec![1];
        data.sign(&keys)?;
        assert_eq!(data.validate_signature(), true);
        Ok(())
    }

    #[test]
    fn max_possible_size() {
        let keys = KeyPair::new();
        let mut data = test_data(&keys);
        data.unknown_data = vec![
            1;
            TelemetryData::SIZE_MASK as usize
                - TelemetryData::serialized_size_of_known_data()
        ];

        assert_deserializable(&Message::TelemetryAck(TelemetryAck(Some(data))));
    }

    #[test]
    fn test_timestamp_accuracy() {
        let before = SystemTime::now();
        sleep(Duration::from_millis(10)); // Small delay to ensure time passes

        let mut telemetry_data = TelemetryData::new();
        telemetry_data.timestamp = SystemTime::now();

        sleep(Duration::from_millis(10)); // Another small delay
        let after = SystemTime::now();

        // Check that the timestamp is between 'before' and 'after'
        assert!(telemetry_data.timestamp >= before);
        assert!(telemetry_data.timestamp <= after);

        // Serialize and deserialize to ensure accuracy is maintained
        let json = serde_json::to_string(&telemetry_data).unwrap();
        let deserialized: TelemetryData = serde_json::from_str(&json).unwrap();

        // Check that the deserialized timestamp is still accurate
        assert!(deserialized.timestamp >= before);
        assert!(deserialized.timestamp <= after);

        // Check that the original and deserialized timestamps are equal
        assert_eq!(telemetry_data.timestamp, deserialized.timestamp);

        // Print time differences for manual verification
        println!(
            "Time from 'before' to timestamp: {:?}",
            telemetry_data.timestamp.duration_since(before).unwrap()
        );
        println!(
            "Time from timestamp to 'after': {:?}",
            after.duration_since(telemetry_data.timestamp).unwrap()
        );
    }

    fn test_data(keys: &KeyPair) -> TelemetryData {
        let mut data = TelemetryData::new();
        data.node_id = keys.public_key().into();
        data.major_version = 20;
        data.minor_version = 1;
        data.patch_version = 5;
        data.pre_release_version = 2;
        data.maker = 1;
        data.timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(100);
        data
    }
}
