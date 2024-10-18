use rsnano_core::{sign_message, utils::MemoryStream, BlockEnum, RawKey};
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, SignArgs, SignDto};
use std::sync::Arc;

pub async fn sign(node: Arc<Node>, args: SignArgs) -> RpcDto {
    let block: BlockEnum = args.block.into();

    let prv = if let Some(key) = args.key {
        key
    } else if let (Some(wallet), Some(account)) = (args.wallet, args.account) {
        match node.wallets.fetch(&wallet, &account.into()) {
            Ok(key) => key,
            Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    } else {
        return RpcDto::Error(ErrorDto::MissingAccountInformation);
    };

    let signature = if prv != RawKey::zero() {
        let mut stream = MemoryStream::new();
        block.serialize(&mut stream);

        let signature = sign_message(&prv, &stream.to_vec());
        signature
    } else {
        return RpcDto::Error(ErrorDto::MissingAccountInformation);
    };

    let sign_dto = SignDto::new(signature, block.json_representation());

    RpcDto::Sign(sign_dto)
}
