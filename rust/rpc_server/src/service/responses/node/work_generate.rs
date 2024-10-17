use rsnano_core::{BlockDetails, BlockEnum, BlockType, DifficultyV1, Epoch, PendingKey};
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, RpcDto, WorkGenerateArgs, WorkGenerateDto, WorkVersionDto};
use std::sync::Arc;

pub async fn work_generate(
    node: Arc<Node>,
    enable_control: bool,
    args: WorkGenerateArgs,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let work_version = args.version.unwrap_or(WorkVersionDto::Work1).into();
    let default_difficulty = node.ledger.constants.work.threshold_base(work_version);
    let mut difficulty = args.difficulty.unwrap_or_else(|| default_difficulty);

    let max_difficulty =
        DifficultyV1::from_multiplier(node.config.max_work_generate_multiplier, default_difficulty);

    // Validate difficulty
    if difficulty > max_difficulty
        || difficulty
            < node
                .network_params
                .work
                .threshold_entry(BlockType::State, work_version)
    {
        return RpcDto::Error(ErrorDto::DifficultyOutOfRange);
    }

    // Handle block if provided
    if let Some(block) = args.block {
        let block_enum: BlockEnum = block.into();
        if args.hash != block_enum.hash() {
            return RpcDto::Error(ErrorDto::BlockRootMismatch);
        }
        if args.version.is_some() && work_version != block_enum.work_version() {
            return RpcDto::Error(ErrorDto::BlockWorkVersioMismatch);
        }
        // Recalculate difficulty if not provided
        if args.difficulty.is_none() && args.multiplier.is_none() {
            difficulty = difficulty_ledger(node.clone(), &block_enum.into());
        }
        /*if node
            .network_params
            .work
            .difficulty(work_version, &args.hash.into(), 0)
            >= difficulty
        {
            return to_string_pretty(&ErrorDto::new(
                "Block work is already sufficient".to_string(),
            ))
            .unwrap();
        }*/
    }

    let use_peers = args.use_peers.unwrap_or(false);
    let mut account = args.account;
    if account.is_none() {
        // Fetch account from block if not given
        account = node
            .ledger
            .any()
            .block_account(&node.ledger.read_txn(), &args.hash);
    }

    //let secondary_work_peers = args.secondary_work_peers.unwrap_or(false);

    let work_result = if !use_peers {
        //if node.distributed_work.work_generation_enabled() {
        node.distributed_work
            .make(args.hash.into(), difficulty, account)
            .await
        /*} else {
            return to_string_pretty(&ErrorDto::new(
                "Local work generation is disabled".to_string(),
            ))
            .unwrap();
        }*/
    } else {
        //if node.distributed_work.work_generation_enabled() {
        node.distributed_work
            .make(args.hash.into(), difficulty, account)
            .await
        /*} else {
            return to_string_pretty(&ErrorDto::new("Work generation is disabled".to_string()))
                .unwrap();
        }*/
    };

    let result_difficulty =
        node.network_params
            .work
            .difficulty(work_version, &args.hash.into(), work_result.unwrap());
    let result_multiplier = DifficultyV1::to_multiplier(
        result_difficulty,
        node.ledger.constants.work.threshold_base(work_version),
    );

    let work_generate_dto = WorkGenerateDto::new(
        work_result.unwrap().into(),
        result_difficulty,
        Some(result_multiplier),
        args.hash,
    );

    RpcDto::WorkGenerate(work_generate_dto)
}

fn difficulty_ledger(node: Arc<Node>, block: &BlockEnum) -> u64 {
    let mut details = BlockDetails::new(Epoch::Epoch0, false, false, false);
    let mut details_found = false;

    let transaction = node.store.tx_begin_read();

    // Previous block find
    let mut block_previous: Option<BlockEnum> = None;
    let previous = block.previous();
    if !previous.is_zero() {
        block_previous = node.ledger.any().get_block(&transaction, &previous);
    }

    // Send check
    if let Some(_prev_block) = &block_previous {
        let is_send =
            node.ledger.any().block_balance(&transaction, &previous) > block.balance_field();
        details = BlockDetails::new(Epoch::Epoch0, is_send, false, false);
        details_found = true;
    }

    // Epoch check
    if let Some(prev_block) = &block_previous {
        let epoch = prev_block.sideband().unwrap().details.epoch;
        details = BlockDetails::new(epoch, details.is_send, details.is_receive, details.is_epoch);
    }

    // Link check
    if let Some(link) = block.link_field() {
        if !details.is_send {
            if let Some(block_link) = node.ledger.any().get_block(&transaction, &link.into()) {
                let account = block.account_field().unwrap();
                if node
                    .ledger
                    .any()
                    .get_pending(&transaction, &PendingKey::new(account, link.into()))
                    .is_some()
                {
                    let epoch =
                        std::cmp::max(details.epoch, block_link.sideband().unwrap().details.epoch);
                    details = BlockDetails::new(epoch, details.is_send, true, details.is_epoch);
                    details_found = true;
                }
            }
        }
    }

    if details_found {
        node.network_params.work.threshold(&details)
    } else {
        node.network_params
            .work
            .threshold_base(block.work_version())
    }
}
