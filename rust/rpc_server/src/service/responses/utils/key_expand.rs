use rsnano_core::{Account, PublicKey, RawKey};
use rsnano_rpc_messages::KeyPairDto;
use serde_json::to_string_pretty;

pub async fn key_expand(key: RawKey) -> String {
    let public: PublicKey = (&key).try_into().unwrap();
    let account = Account::from(public);

    to_string_pretty(&KeyPairDto::new(key, public, account)).unwrap()
}