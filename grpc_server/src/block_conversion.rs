use rsnano_grpc_proto::nano::v1::{
    BlockLocalState, BlockRecord, BlockSideband, BlockSubtype, Epoch as ProtoEpoch,
    LegacyChangeBlock, LegacyOpenBlock, LegacyReceiveBlock, LegacySendBlock, NanoBlock, StateBlock,
    nano_block,
};
use rsnano_ledger::{AnySet, Ledger, LedgerSet};
use rsnano_types::{Block, BlockBase, BlockHash, BlockType, Epoch, SavedBlock};

pub(crate) fn block_record(ledger: &Ledger, block: &SavedBlock) -> BlockRecord {
    let any = ledger.any();
    let sideband = block.sideband();
    let local_state = if any.confirmed().block_exists(&block.hash()) {
        BlockLocalState::Cemented
    } else {
        BlockLocalState::Processed
    };

    BlockRecord {
        block_hash: block.hash().to_string(),
        block: Some(nano_block(block)),
        sideband: Some(BlockSideband {
            account: block.account().encode_account(),
            account_height: sideband.height,
            local_timestamp_unix_ms: sideband.timestamp.as_u64(),
            resulting_balance_raw: block.balance().to_string_dec(),
            epoch: proto_epoch(sideband.details.epoch) as i32,
            source_epoch: proto_epoch(sideband.source_epoch) as i32,
            subtype: block_subtype(block) as i32,
        }),
        amount_raw: any
            .block_amount_for(block)
            .map(|amount| amount.to_string_dec())
            .unwrap_or_default(),
        linked_account: any
            .linked_account(block)
            .map(|account| account.encode_account()),
        local_state: local_state as i32,
    }
}

pub(crate) fn block_status(ledger: &Ledger, hash: &BlockHash) -> BlockLocalState {
    let any = ledger.any();
    if any.confirmed().block_exists(hash) {
        BlockLocalState::Cemented
    } else if any.block_exists(hash) {
        BlockLocalState::Processed
    } else {
        BlockLocalState::NotPresent
    }
}

fn nano_block(block: &Block) -> NanoBlock {
    let kind = match block {
        Block::State(block) => nano_block::Kind::State(StateBlock {
            account: block.account().encode_account(),
            previous: block.previous().to_string(),
            representative: block.representative().as_account().encode_account(),
            balance_raw: block.balance().to_string_dec(),
            link: block.link().to_string(),
            signature: block.signature().encode_hex(),
            work: block.work().to_string(),
        }),
        Block::LegacySend(block) => nano_block::Kind::LegacySend(LegacySendBlock {
            previous: block.previous().to_string(),
            destination: block.destination().encode_account(),
            balance_raw: block.balance().to_string_dec(),
            signature: block.signature().encode_hex(),
            work: block.work().to_string(),
        }),
        Block::LegacyReceive(block) => nano_block::Kind::LegacyReceive(LegacyReceiveBlock {
            previous: block.previous().to_string(),
            source: block.source().to_string(),
            signature: block.signature().encode_hex(),
            work: block.work().to_string(),
        }),
        Block::LegacyOpen(block) => nano_block::Kind::LegacyOpen(LegacyOpenBlock {
            source: block.source().to_string(),
            representative: block.representative().as_account().encode_account(),
            account: block.account().encode_account(),
            signature: block.signature().encode_hex(),
            work: block.work().to_string(),
        }),
        Block::LegacyChange(block) => nano_block::Kind::LegacyChange(LegacyChangeBlock {
            previous: block.previous().to_string(),
            representative: block
                .mandatory_representative()
                .as_account()
                .encode_account(),
            signature: block.signature().encode_hex(),
            work: block.work().to_string(),
        }),
    };
    NanoBlock { kind: Some(kind) }
}

fn block_subtype(block: &SavedBlock) -> BlockSubtype {
    match block.block_type() {
        BlockType::LegacySend => BlockSubtype::Send,
        BlockType::LegacyReceive => BlockSubtype::Receive,
        BlockType::LegacyOpen => BlockSubtype::Open,
        BlockType::LegacyChange => BlockSubtype::Change,
        BlockType::State => {
            if block.sideband().details.is_send {
                BlockSubtype::Send
            } else if block.sideband().details.is_receive {
                if block.previous().is_zero() {
                    BlockSubtype::Open
                } else {
                    BlockSubtype::Receive
                }
            } else if block.sideband().details.is_epoch {
                BlockSubtype::Epoch
            } else {
                BlockSubtype::Change
            }
        }
    }
}

pub(crate) fn proto_epoch(epoch: Epoch) -> ProtoEpoch {
    match epoch {
        Epoch::Epoch1 => ProtoEpoch::Epoch1,
        Epoch::Epoch2 => ProtoEpoch::Epoch2,
        Epoch::Epoch0 => ProtoEpoch::Epoch0,
        Epoch::Invalid | Epoch::Unspecified => ProtoEpoch::Unspecified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_grpc_proto::nano::v1::nano_block::Kind;
    use rsnano_types::{ChangeBlock, OpenBlock, ReceiveBlock, SendBlock};

    #[test]
    fn preserves_every_historical_wire_block_variant() {
        let cases = [
            (Block::new_test_instance(), "state"),
            (Block::LegacySend(SendBlock::new_test_instance()), "send"),
            (
                Block::LegacyReceive(ReceiveBlock::new_test_instance()),
                "receive",
            ),
            (Block::LegacyOpen(OpenBlock::new_test_instance()), "open"),
            (
                Block::LegacyChange(ChangeBlock::new_test_instance()),
                "change",
            ),
        ];

        for (block, expected) in cases {
            let actual = match nano_block(&block).kind.expect("block kind") {
                Kind::State(_) => "state",
                Kind::LegacySend(_) => "send",
                Kind::LegacyReceive(_) => "receive",
                Kind::LegacyOpen(_) => "open",
                Kind::LegacyChange(_) => "change",
            };
            assert_eq!(actual, expected);
        }
    }
}
