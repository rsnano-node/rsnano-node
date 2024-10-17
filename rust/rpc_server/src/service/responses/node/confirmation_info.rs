use rsnano_core::Amount;
use rsnano_node::Node;
use rsnano_rpc_messages::{
    ConfirmationBlockInfoDto, ConfirmationInfoArgs, ConfirmationInfoDto, ErrorDto, RpcDto,
};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn confirmation_info(node: Arc<Node>, args: ConfirmationInfoArgs) -> RpcDto {
    let election = node.active.election(&args.root);

    if let Some(election) = election {
        if !node.active.confirmed(&election) {
            let info = election.mutex.lock().unwrap();
            let mut blocks = HashMap::new();
            let mut total_tally = Amount::zero();

            for (hash, block) in info.last_blocks.iter() {
                let tally = info.last_tally.get(hash).cloned().unwrap_or(Amount::zero());
                let mut block_info = ConfirmationBlockInfoDto {
                    tally,
                    contents: None,
                    representatives: None,
                };

                if args.contents.unwrap_or(true) {
                    block_info.contents = Some(block.json_representation());
                }

                if args.representatives.unwrap_or(false) {
                    let mut reps = HashMap::new();
                    for (representative, vote) in &info.last_votes {
                        if hash == &vote.hash {
                            let amount = node.ledger.rep_weights.weight(representative);
                            reps.insert(representative.clone().into(), amount);
                        }
                    }
                    block_info.representatives = Some(reps);
                }

                total_tally += tally;
                blocks.insert(*hash, block_info);
            }

            let confirmation_info_dto = ConfirmationInfoDto::new(
                info.status.confirmation_request_count,
                info.last_votes.len(),
                info.status
                    .winner
                    .as_ref()
                    .map(|w| w.hash())
                    .unwrap_or_default(),
                total_tally,
                info.status.final_tally,
                blocks,
            );

            RpcDto::ConfirmationInfo(confirmation_info_dto)
        } else {
            RpcDto::Error(ErrorDto::ConfirmationInfoNotFound)
        }
    } else {
        RpcDto::Error(ErrorDto::InvalidRoot)
    }
}
