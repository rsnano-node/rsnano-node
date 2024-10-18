use rsnano_core::PendingKey;
use rsnano_node::{Node, NodeExt};
use rsnano_rpc_messages::{BlockHashesDto, ErrorDto, RepublishArgs, RpcDto};
use std::sync::Arc;
use std::time::Duration;

pub async fn republish(node: Arc<Node>, args: RepublishArgs) -> RpcDto {
    let mut blocks = Vec::new();
    let transaction = node.store.tx_begin_read();
    let count = args.count.unwrap_or(1024);

    if let Some(mut block) = node.ledger.any().get_block(&transaction, &args.hash) {
        let mut republish_bundle = Vec::new();

        for _ in 0..count {
            if args.hash.is_zero() {
                break;
            }

            // Handle sources
            if let Some(sources_count) = args.sources {
                let source = block
                    .source_field()
                    .or_else(|| block.link_field().map(|link| link.into()))
                    .unwrap_or_default();
                let mut source_block = node.ledger.any().get_block(&transaction, &source);
                let mut source_hashes = Vec::new();

                while let Some(sb) = source_block {
                    if source_hashes.len() >= sources_count as usize {
                        break;
                    }
                    source_hashes.push(sb.hash());
                    let previous = sb.previous();
                    source_block = node.ledger.any().get_block(&transaction, &previous);
                }

                for hash in source_hashes.into_iter().rev() {
                    if let Some(b) = node.ledger.any().get_block(&transaction, &hash) {
                        republish_bundle.push(b.clone());
                        blocks.push(hash);
                    }
                }
            }

            // Add the current block
            republish_bundle.push(block.clone());
            blocks.push(args.hash);

            // Handle destinations
            if let Some(destinations_count) = args.destinations {
                if let Some(destination) = block.destination() {
                    if !node
                        .ledger
                        .any()
                        .get_pending(&transaction, &PendingKey::new(destination, args.hash))
                        .is_some()
                    {
                        let mut previous =
                            match node.ledger.any().account_head(&transaction, &destination) {
                                Some(block_hash) => block_hash,
                                None => return RpcDto::Error(ErrorDto::AccountHeadNotFound),
                            };
                        let mut dest_block = node.ledger.any().get_block(&transaction, &previous);
                        let mut dest_hashes = Vec::new();

                        while let Some(db) = dest_block {
                            if dest_hashes.len() >= destinations_count as usize {
                                break;
                            }
                            dest_hashes.push(previous);
                            let source = db
                                .source_field()
                                .or_else(|| {
                                    if db.is_send() {
                                        None
                                    } else {
                                        db.link_field().map(|link| link.into())
                                    }
                                })
                                .unwrap_or_default();
                            if args.hash == source {
                                break;
                            }
                            previous = db.previous();
                            dest_block = node.ledger.any().get_block(&transaction, &previous);
                        }

                        for hash in dest_hashes.into_iter().rev() {
                            if let Some(b) = node.ledger.any().get_block(&transaction, &hash) {
                                republish_bundle.push(b.clone());
                                blocks.push(hash);
                            }
                        }
                    }
                }
            }

            // Move to the next block
            let next_hash = node
                .ledger
                .any()
                .block_successor(&transaction, &args.hash)
                .unwrap_or_default();
            if let Some(next_block) = node.ledger.any().get_block(&transaction, &next_hash) {
                block = next_block;
            } else {
                break;
            }
        }

        // Flood the network with republished blocks
        node.flood_block_many(
            republish_bundle.into(),
            Box::new(|| {}),
            Duration::from_millis(25),
        );
    }

    RpcDto::Republish(BlockHashesDto::new(blocks))
}
