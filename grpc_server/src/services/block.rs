use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    BlockStatus, GetBlockRequest, GetBlockResponse, GetBlockStatusesRequest,
    GetBlockStatusesResponse, PublishDisposition, PublishStateBlockRequest,
    PublishStateBlockResponse, RequestBlockConfirmationRequest, RequestBlockConfirmationResponse,
    StateBlock as ProtoStateBlock, block_service_server::BlockService,
};
use rsnano_ledger::{AnySet, BlockError, LedgerSet};
use rsnano_node::Node;
use rsnano_types::{Account, Amount, Block, BlockHash, JsonStateBlock, Link, Signature, WorkNonce};
use tonic::{Request, Response, Status};

use crate::block_conversion::{block_record, block_status};

pub struct BlockServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl BlockService for BlockServiceImpl {
    async fn publish_state_block(
        &self,
        request: Request<PublishStateBlockRequest>,
    ) -> Result<Response<PublishStateBlockResponse>, Status> {
        let block = decode_state_block(
            request
                .into_inner()
                .block
                .ok_or_else(|| Status::invalid_argument("state block is required"))?,
        )?;
        let hash = block.hash();
        let disposition = match self.node.process_local(block) {
            Ok(_) => PublishDisposition::Accepted,
            Err(BlockError::Old(_)) => PublishDisposition::AlreadyPresent,
            Err(error) => return Err(map_block_error(error)),
        };

        Ok(Response::new(PublishStateBlockResponse {
            block_hash: hash.to_string(),
            disposition: disposition as i32,
        }))
    }

    async fn get_block(
        &self,
        request: Request<GetBlockRequest>,
    ) -> Result<Response<GetBlockResponse>, Status> {
        let hash = decode_hash(&request.into_inner().block_hash)?;
        let block = self
            .node
            .ledger
            .any()
            .get_block(&hash)
            .ok_or_else(|| Status::not_found("block not found"))?;
        Ok(Response::new(GetBlockResponse {
            block: Some(block_record(&self.node.ledger, &block)),
        }))
    }

    async fn get_block_statuses(
        &self,
        request: Request<GetBlockStatusesRequest>,
    ) -> Result<Response<GetBlockStatusesResponse>, Status> {
        let statuses = request
            .into_inner()
            .block_hashes
            .into_iter()
            .map(|value| {
                let hash = decode_hash(&value)?;
                Ok(BlockStatus {
                    block_hash: hash.to_string(),
                    local_state: block_status(&self.node.ledger, &hash) as i32,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        Ok(Response::new(GetBlockStatusesResponse { statuses }))
    }

    async fn request_block_confirmation(
        &self,
        request: Request<RequestBlockConfirmationRequest>,
    ) -> Result<Response<RequestBlockConfirmationResponse>, Status> {
        let hash = decode_hash(&request.into_inner().block_hash)?;
        let block = self
            .node
            .ledger
            .any()
            .get_block(&hash)
            .ok_or_else(|| Status::failed_precondition("block is not in the local ledger"))?;
        let already_cemented = self.node.ledger.confirmed().block_exists(&hash);
        let already_scheduled = self.node.election_schedulers.contains(&hash)
            || self.node.confirming_set.contains(&hash);
        if !already_cemented && !already_scheduled {
            self.node.election_schedulers.add_manual(block);
        }
        Ok(Response::new(RequestBlockConfirmationResponse {
            election_requested: !already_cemented,
        }))
    }
}

fn decode_state_block(value: ProtoStateBlock) -> Result<Block, Status> {
    let account = Account::parse(&value.account)
        .ok_or_else(|| Status::invalid_argument("invalid account"))?;
    let previous = decode_hash_named(&value.previous, "previous")?;
    let representative = Account::parse(&value.representative)
        .ok_or_else(|| Status::invalid_argument("invalid representative"))?
        .as_key();
    let balance = Amount::decode_dec(&value.balance_raw)
        .map_err(|_| Status::invalid_argument("invalid balance_raw"))?;
    let link = Link::decode_hex(&value.link)
        .filter(|_| is_uppercase_hex(&value.link, 64))
        .ok_or_else(|| {
            Status::invalid_argument("link must be 64 uppercase hexadecimal characters")
        })?;
    let signature = Signature::decode_hex(&value.signature)
        .filter(|_| is_hex(&value.signature, 128))
        .ok_or_else(|| Status::invalid_argument("signature must be 128 hexadecimal characters"))?;
    let work = u64::from_str_radix(&value.work, 16)
        .ok()
        .filter(|_| is_hex(&value.work, 16))
        .ok_or_else(|| Status::invalid_argument("work must be 16 hexadecimal characters"))?;

    Ok(Block::State(rsnano_types::StateBlock::from(
        JsonStateBlock {
            account,
            previous,
            representative: representative.as_account(),
            balance,
            link,
            link_as_account: None,
            signature,
            work: WorkNonce::new(work),
        },
    )))
}

pub(crate) fn decode_hash(value: &str) -> Result<BlockHash, Status> {
    decode_hash_named(value, "block_hash")
}

fn decode_hash_named(value: &str, name: &str) -> Result<BlockHash, Status> {
    BlockHash::decode_hex(value)
        .filter(|_| is_uppercase_hex(value, 64))
        .ok_or_else(|| {
            Status::invalid_argument(format!(
                "{name} must be 64 uppercase hexadecimal characters"
            ))
        })
}

fn is_uppercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn map_block_error(error: BlockError) -> Status {
    match error {
        BlockError::Old(_) => Status::already_exists("block already exists"),
        BlockError::Fork | BlockError::Conflict => {
            Status::aborted("block conflicts with the account chain")
        }
        BlockError::GapPrevious | BlockError::GapSource | BlockError::GapEpochOpenPending => {
            Status::failed_precondition("required ledger prerequisite is missing")
        }
        BlockError::BadSignature
        | BlockError::NegativeSpend
        | BlockError::Unreceivable
        | BlockError::OpenedBurnAccount
        | BlockError::BalanceMismatch
        | BlockError::RepresentativeMismatch
        | BlockError::BlockPosition
        | BlockError::InsufficientWork => Status::invalid_argument(error.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_types::{BlockBase, PrivateKey, StateBlockArgs};

    #[test]
    fn decodes_signed_state_block_and_preserves_its_hash() {
        let key = PrivateKey::from(1);
        let native = Block::from(StateBlockArgs {
            key: &key,
            previous: BlockHash::ZERO,
            representative: key.public_key(),
            balance: Amount::raw(7),
            link: Link::ZERO,
            work: WorkNonce::new(0),
        });
        let Block::State(state) = native.clone() else {
            unreachable!()
        };
        let decoded = decode_state_block(ProtoStateBlock {
            account: state.account().encode_account(),
            previous: state.previous().to_string(),
            representative: state.representative().as_account().encode_account(),
            balance_raw: state.balance().to_string_dec(),
            link: state.link().to_string(),
            signature: state.signature().encode_hex(),
            work: state.work().to_string(),
        })
        .unwrap();
        assert_eq!(decoded.hash(), native.hash());
    }

    #[test]
    fn rejects_noncanonical_hash_encoding() {
        assert_eq!(
            decode_hash(&"a".repeat(64)).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn maps_ledger_errors_to_public_statuses() {
        assert_eq!(
            map_block_error(BlockError::BadSignature).code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            map_block_error(BlockError::GapPrevious).code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            map_block_error(BlockError::Fork).code(),
            tonic::Code::Aborted
        );
    }
}
