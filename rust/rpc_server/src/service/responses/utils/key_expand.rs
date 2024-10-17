use rsnano_core::{Account, PublicKey};
use rsnano_rpc_messages::{KeyExpandArgs, KeyPairDto, RpcDto};

pub async fn key_expand(args: KeyExpandArgs) -> RpcDto {
    let public: PublicKey = (&args.key).try_into().unwrap();
    let account = Account::from(public);

    RpcDto::KeyExpand(KeyPairDto::new(args.key, public, account))
}
