use rsnano_core::{Account, KeyPair};
use rsnano_rpc_messages::{KeyPairDto, RpcDto};

pub async fn key_create() -> RpcDto {
    let keypair = KeyPair::new();
    let private = keypair.private_key();
    let public = keypair.public_key();
    let account = Account::from(public);

    RpcDto::KeyPair(KeyPairDto::new(private, public, account))
}
