use super::Account;
use crate::u256_struct;
use blake2::digest::Update;
use blake2::digest::VariableOutput;
use blake2::Blake2bVar;
use rand::thread_rng;
use rand::Rng;

u256_struct!(BlockHash);

impl BlockHash {
    pub fn random() -> Self {
        BlockHash::from_bytes(thread_rng().gen())
    }
}

impl serde::Serialize for BlockHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_hex())
    }
}

impl From<&Account> for BlockHash {
    fn from(account: &Account) -> Self {
        Self::from_bytes(*account.as_bytes())
    }
}

impl From<Account> for BlockHash {
    fn from(account: Account) -> Self {
        Self::from_bytes(*account.as_bytes())
    }
}

pub struct BlockHashBuilder {
    blake: Blake2bVar,
}

impl Default for BlockHashBuilder {
    fn default() -> Self {
        Self {
            blake: Blake2bVar::new(32).unwrap(),
        }
    }
}

impl BlockHashBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(mut self, data: impl AsRef<[u8]>) -> Self {
        self.blake.update(data.as_ref());
        self
    }

    pub fn build(self) -> BlockHash {
        let mut hash_bytes = [0u8; 32];
        self.blake.finalize_variable(&mut hash_bytes).unwrap();
        BlockHash::from_bytes(hash_bytes)
    }
}