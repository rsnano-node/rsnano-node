use rsnano_core::{Account, PublicKey};
use rsnano_rpc_messages::{DeterministicKeyArgs, KeyPairDto, RpcDto};

pub async fn deterministic_key(args: DeterministicKeyArgs) -> RpcDto {
    let private = rsnano_core::deterministic_key(&args.seed, args.index);
    let public: PublicKey = (&private).try_into().unwrap();
    let account = Account::from(public);

    RpcDto::DeterministicKey(KeyPairDto::new(private, public, account))
}
