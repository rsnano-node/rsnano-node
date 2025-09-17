#[macro_use]
extern crate num_derive;

#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate strum_macros;

mod account;
mod account_info;
mod amount;
mod block_hash;
mod blocks;
mod confirmation_height_info;
mod difficulty;
mod epoch;
mod kdf;
mod node_id;
mod peer;
mod pending_info;
mod pending_key;
mod priority;
mod private_key;
mod public_key;
mod qualified_root;
mod raw_key;
mod signature;
mod timestamp;
mod u256_struct;
mod vote;
mod vote_timestamp;

use std::{
    fmt::{Debug, Display},
    io::Read,
    str::FromStr,
    sync::{Arc, Condvar, Mutex},
};

pub use account::Account;
pub use account_info::AccountInfo;
pub use amount::{Amount, DescTallyKey};
use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
pub use block_hash::{Blake2Hash, Blake2HashBuilder, BlockHash};
pub use blocks::*;
pub use confirmation_height_info::ConfirmationHeightInfo;
pub use difficulty::{Difficulty, DifficultyV1, StubDifficulty};
pub use epoch::*;
pub use kdf::KeyDerivationFunction;
pub use node_id::NodeId;
pub use peer::Peer;
pub use pending_info::PendingInfo;
pub use pending_key::PendingKey;
pub use priority::{BlockPriority, TimePriority};
pub use private_key::{PrivateKey, PrivateKeyFactory};
pub use public_key::{PublicKey, SignatureError};
pub use qualified_root::QualifiedRoot;
pub use raw_key::RawKey;
use serde::de::{Unexpected, Visitor};
pub use signature::Signature;
use thiserror::Error;
pub use timestamp::{UnixMillisTimestamp, UnixTimestamp, milliseconds_since_epoch};
pub use vote::{TestVoteBuilder, Vote, VoteError, VoteSource};
pub use vote_timestamp::VoteTimestamp;

pub type SnapshotNumber = u32;

pub fn write_hex_bytes(bytes: &[u8], f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
    for &byte in bytes {
        write!(f, "{:02X}", byte)?;
    }
    Ok(())
}

pub fn to_hex_string(i: u64) -> String {
    format!("{:016X}", i)
}

u256_struct!(HashOrAccount);
serialize_32_byte_string!(HashOrAccount);
u256_struct!(Link);
serialize_32_byte_string!(Link);
u256_struct!(Root);
serialize_32_byte_string!(Root);
u256_struct!(WalletId);
serialize_32_byte_string!(WalletId);

impl WalletId {
    pub fn random() -> Self {
        let key = PrivateKey::new();
        Self::from_bytes(*key.public_key().as_bytes())
    }
}

impl From<HashOrAccount> for Account {
    fn from(source: HashOrAccount) -> Self {
        Account::from_bytes(*source.as_bytes())
    }
}

impl From<&HashOrAccount> for Account {
    fn from(source: &HashOrAccount) -> Self {
        Account::from_bytes(*source.as_bytes())
    }
}

impl From<Link> for Account {
    fn from(link: Link) -> Self {
        Account::from_bytes(*link.as_bytes())
    }
}

impl From<&Link> for Account {
    fn from(link: &Link) -> Self {
        Account::from_bytes(*link.as_bytes())
    }
}

impl From<Root> for Account {
    fn from(root: Root) -> Self {
        Account::from_bytes(*root.as_bytes())
    }
}

impl From<Account> for Link {
    fn from(account: Account) -> Self {
        Link::from_bytes(*account.as_bytes())
    }
}

impl From<&Account> for Link {
    fn from(account: &Account) -> Self {
        Link::from_bytes(*account.as_bytes())
    }
}

impl From<BlockHash> for Link {
    fn from(hash: BlockHash) -> Self {
        Link::from_bytes(*hash.as_bytes())
    }
}

impl From<HashOrAccount> for BlockHash {
    fn from(source: HashOrAccount) -> Self {
        BlockHash::from_bytes(*source.as_bytes())
    }
}

impl From<&HashOrAccount> for BlockHash {
    fn from(source: &HashOrAccount) -> Self {
        BlockHash::from_bytes(*source.as_bytes())
    }
}
impl From<Link> for BlockHash {
    fn from(link: Link) -> Self {
        BlockHash::from_bytes(*link.as_bytes())
    }
}

impl From<Root> for BlockHash {
    fn from(root: Root) -> Self {
        BlockHash::from_bytes(*root.as_bytes())
    }
}

impl From<Account> for HashOrAccount {
    fn from(account: Account) -> Self {
        HashOrAccount::from_bytes(*account.as_bytes())
    }
}

impl From<&BlockHash> for HashOrAccount {
    fn from(hash: &BlockHash) -> Self {
        HashOrAccount::from_bytes(*hash.as_bytes())
    }
}

impl From<BlockHash> for HashOrAccount {
    fn from(hash: BlockHash) -> Self {
        HashOrAccount::from_bytes(*hash.as_bytes())
    }
}

impl From<Link> for HashOrAccount {
    fn from(link: Link) -> Self {
        HashOrAccount::from_bytes(*link.as_bytes())
    }
}

impl From<&Link> for HashOrAccount {
    fn from(link: &Link) -> Self {
        HashOrAccount::from_bytes(*link.as_bytes())
    }
}

impl From<Account> for Root {
    fn from(key: Account) -> Self {
        Root::from_bytes(*key.as_bytes())
    }
}

impl From<&PublicKey> for Root {
    fn from(key: &PublicKey) -> Self {
        Root::from_bytes(*key.as_bytes())
    }
}

impl From<PublicKey> for Root {
    fn from(key: PublicKey) -> Self {
        Root::from_bytes(*key.as_bytes())
    }
}

impl From<&Account> for Root {
    fn from(hash: &Account) -> Self {
        Root::from_bytes(*hash.as_bytes())
    }
}

impl From<BlockHash> for Root {
    fn from(hash: BlockHash) -> Self {
        Root::from_bytes(*hash.as_bytes())
    }
}

impl From<&BlockHash> for Root {
    fn from(hash: &BlockHash) -> Self {
        Root::from_bytes(*hash.as_bytes())
    }
}

pub fn deterministic_key(seed: &RawKey, index: u32) -> RawKey {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(seed.as_bytes());
    hasher.update(&index.to_be_bytes());

    let mut buffer = [0; 32];
    hasher.finalize_variable(&mut buffer).unwrap();
    RawKey::from_bytes(buffer)
}

/**
 * Network variants with different genesis blocks and network parameters
 */
#[repr(u16)]
#[derive(Clone, Copy, FromPrimitive, PartialEq, Eq, Debug)]
pub enum Networks {
    Invalid = 0x0,
    // Low work parameters, publicly known genesis key, dev IP ports
    NanoDevNetwork = 0x5241, // 'R', 'A'
    // Normal work parameters, secret beta genesis key, beta IP ports
    NanoBetaNetwork = 0x5242, // 'R', 'B'
    // Normal work parameters, secret live key, live IP ports
    NanoLiveNetwork = 0x5243, // 'R', 'C'
    // Normal work parameters, secret test genesis key, test IP ports
    NanoTestNetwork = 0x5258, // 'R', 'X'
}

impl Networks {
    pub fn as_str(&self) -> &str {
        match self {
            Networks::Invalid => "invalid",
            Networks::NanoDevNetwork => "dev",
            Networks::NanoBetaNetwork => "beta",
            Networks::NanoLiveNetwork => "live",
            Networks::NanoTestNetwork => "test",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct ProtocolInfo {
    pub version_using: u8,
    pub version_max: u8,
    pub version_min: u8,
    pub network: Networks,
}

impl Default for ProtocolInfo {
    fn default() -> Self {
        Self {
            version_using: 0x15,
            version_max: 0x15,
            version_min: 0x14,
            network: Networks::NanoLiveNetwork,
        }
    }
}

impl ProtocolInfo {
    pub fn default_for(network: Networks) -> Self {
        Self {
            network,
            ..Default::default()
        }
    }
}

impl FromStr for Networks {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Networks, Self::Err> {
        match s {
            "dev" => Ok(Networks::NanoDevNetwork),
            "beta" => Ok(Networks::NanoBetaNetwork),
            "live" => Ok(Networks::NanoLiveNetwork),
            "test" => Ok(Networks::NanoTestNetwork),
            _ => Err("Invalid network"),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Default, Clone)]
pub struct Frontier {
    pub account: Account,
    pub hash: BlockHash,
}

impl Frontier {
    pub fn new(account: Account, hash: BlockHash) -> Self {
        Self { account, hash }
    }

    pub fn new_test_instance() -> Self {
        Self::new(Account::from(1), BlockHash::from(2))
    }

    pub fn serialize<T>(&self, writer: &mut T) -> std::io::Result<()>
    where
        T: std::io::Write,
    {
        self.account.serialize(writer)?;
        self.hash.serialize(writer)
    }

    pub fn deserialize<T>(reader: &mut T) -> Result<Self, DeserializationError>
    where
        T: Read,
    {
        let account = Account::deserialize(reader)?;
        let hash = BlockHash::deserialize(reader)?;
        Ok(Self::new(account, hash))
    }
}

#[derive(PartialEq, Eq, Copy, Clone, PartialOrd, Ord, Default)]
pub struct WorkNonce(u64);

impl WorkNonce {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl Display for WorkNonce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016X}", self.0)
    }
}

impl Debug for WorkNonce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorkRequest {
    pub root: Root,
    pub difficulty: u64,
}

impl WorkRequest {
    pub fn new(root: Root, difficulty: u64) -> Self {
        Self { root, difficulty }
    }

    pub fn new_test_instance() -> Self {
        Self::new(Root::from(100), 0)
    }

    pub fn difficulty_of(&self, work: WorkNonce) -> u64 {
        DifficultyV1 {}.get_difficulty(&self.root, work)
    }

    pub fn with_callback(
        self,
        done: Box<dyn FnOnce(Option<WorkNonce>) + Send>,
    ) -> WorkRequestAsync {
        WorkRequestAsync::new(self.root, self.difficulty, done)
    }

    pub fn into_async(self) -> (WorkRequestAsync, WorkDoneNotifier) {
        let done = WorkDoneNotifier::new();
        let done2 = done.clone();
        let req = self.with_callback(Box::new(move |work| done2.signal_done(work)));
        (req, done)
    }
}

pub struct WorkRequestAsync {
    pub root: Root,
    pub difficulty: u64,
    pub done: Option<Box<dyn FnOnce(Option<WorkNonce>) + Send>>,
}

impl WorkRequestAsync {
    pub fn new(
        root: Root,
        difficulty: u64,
        done: Box<dyn FnOnce(Option<WorkNonce>) + Send>,
    ) -> Self {
        Self {
            root,
            difficulty,
            done: Some(done),
        }
    }

    pub fn work_found(mut self, work: WorkNonce) {
        // we're the ones that found the solution
        if let Some(callback) = self.done.take() {
            (callback)(Some(work));
        }
    }

    pub fn cancelled(mut self) {
        if let Some(callback) = self.done.take() {
            (callback)(None);
        }
    }

    pub fn request(&self) -> WorkRequest {
        WorkRequest::new(self.root, self.difficulty)
    }

    pub fn is_valid_work(&self, work: WorkNonce) -> bool {
        self.difficulty_of(work) >= self.difficulty
    }

    pub fn difficulty_of(&self, work: WorkNonce) -> u64 {
        DifficultyV1 {}.get_difficulty(&self.root, work)
    }
}

#[derive(Default)]
struct WorkDoneState {
    work: Option<WorkNonce>,
    done: bool,
}

#[derive(Clone)]
pub struct WorkDoneNotifier {
    state: Arc<(Mutex<WorkDoneState>, Condvar)>,
}

impl Default for WorkDoneNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkDoneNotifier {
    pub fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(WorkDoneState::default()), Condvar::new())),
        }
    }

    pub fn signal_done(&self, work: Option<WorkNonce>) {
        {
            let mut lock = self.state.0.lock().unwrap();
            lock.work = work;
            lock.done = true;
        }
        self.state.1.notify_one();
    }

    pub fn wait(&self) -> Option<WorkNonce> {
        let mut lock = self.state.0.lock().unwrap();
        loop {
            if lock.done {
                return lock.work;
            }
            lock = self.state.1.wait(lock).unwrap();
        }
    }
}

impl From<u64> for WorkNonce {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<WorkNonce> for u64 {
    fn from(value: WorkNonce) -> Self {
        value.0
    }
}

impl serde::Serialize for WorkNonce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&to_hex_string(self.0))
    }
}

impl<'de> serde::Deserialize<'de> for WorkNonce {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = deserializer.deserialize_str(WorkNonceVisitor {})?;
        Ok(value)
    }
}

struct WorkNonceVisitor {}

impl Visitor<'_> for WorkNonceVisitor {
    type Value = WorkNonce;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a hex string containing 8 bytes")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let mut bytes = [0; 8];
        hex::decode_to_slice(v, &mut bytes).map_err(|_| {
            serde::de::Error::invalid_value(Unexpected::Str(v), &"a hex string containing 8 bytes")
        })?;
        Ok(WorkNonce(u64::from_be_bytes(bytes)))
    }
}

#[derive(Error, Debug)]
pub enum DeserializationError {
    #[error("invalid data")]
    InvalidData,

    #[error("too much data")]
    TooMuchData,

    #[error("I/O error")]
    IoError(std::io::Error),
}

impl From<std::io::Error> for DeserializationError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

pub fn read_u64_le<T>(reader: &mut T) -> std::io::Result<u64>
where
    T: Read,
{
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(u64::from_le_bytes(buffer))
}

pub fn read_u64_be<T>(reader: &mut T) -> std::io::Result<u64>
where
    T: Read,
{
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(u64::from_be_bytes(buffer))
}

pub fn read_u64_ne<T>(reader: &mut T) -> std::io::Result<u64>
where
    T: Read,
{
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(u64::from_ne_bytes(buffer))
}

pub fn read_u32_be<T>(reader: &mut T) -> std::io::Result<u32>
where
    T: Read,
{
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)?;
    Ok(u32::from_be_bytes(buffer))
}

pub fn read_u8<T>(reader: &mut T) -> std::io::Result<u8>
where
    T: Read,
{
    let mut buffer = [0; 1];
    reader.read_exact(&mut buffer)?;
    Ok(buffer[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_key() {
        let seed = RawKey::from(1);
        let key = deterministic_key(&seed, 3);
        assert_eq!(
            key,
            RawKey::decode_hex("89A518E3B70A0843DE8470F87FF851F9C980B1B2802267A05A089677B8FA1926")
                .unwrap()
        );
    }

    #[test]
    fn serialize_work_nonce() {
        let serialized = serde_json::to_string(&WorkNonce::from(123)).unwrap();
        assert_eq!(serialized, "\"000000000000007B\"");
    }
}
