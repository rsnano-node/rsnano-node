use rsnano_core::{Amount, BlockHash};
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, ReceivableDto, RpcDto, SourceInfo, WalletReceivableArgs};
use std::{collections::HashMap, sync::Arc};

pub async fn wallet_receivable(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletReceivableArgs,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let accounts = match node.wallets.get_accounts_of_wallet(&args.wallet) {
        Ok(accounts) => accounts,
        Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
    };

    let tx = node.ledger.read_txn();
    let mut block_source = HashMap::new();
    let mut block_threshold = HashMap::new();
    let mut block_default = HashMap::new();

    for account in accounts {
        let mut account_blocks_source: HashMap<BlockHash, SourceInfo> = HashMap::new();
        let mut account_blocks_threshold: HashMap<BlockHash, Amount> = HashMap::new();
        let mut account_blocks_default: Vec<BlockHash> = Vec::new();
        for (key, info) in node
            .ledger
            .any()
            .account_receivable_upper_bound(&tx, account, BlockHash::zero())
            .take(args.count as usize)
        {
            if args.include_only_confirmed.unwrap_or(true)
                && !node
                    .ledger
                    .confirmed()
                    .block_exists_or_pruned(&tx, &key.send_block_hash)
            {
                continue;
            }

            if let Some(threshold) = args.threshold {
                if info.amount < threshold {
                    continue;
                }
            }

            if args.source.unwrap_or(false) || args.min_version.unwrap_or(false) {
                let source_info = SourceInfo {
                    amount: info.amount,
                    source: info.source,
                };
                account_blocks_source.insert(key.send_block_hash, source_info);
            } else if args.threshold.is_some() {
                account_blocks_threshold.insert(key.send_block_hash, info.amount);
            } else {
                account_blocks_default.push(key.send_block_hash);
            }
        }

        if !account_blocks_source.is_empty() {
            block_source.insert(account, account_blocks_source);
        }
        if !account_blocks_threshold.is_empty() {
            block_threshold.insert(account, account_blocks_threshold);
        }
        if !account_blocks_default.is_empty() {
            block_default.insert(account, account_blocks_default);
        }
    }

    let result = if args.source.unwrap_or(false) || args.min_version.unwrap_or(false) {
        ReceivableDto::Source {
            blocks: block_source,
        }
    } else if args.threshold.is_some() {
        ReceivableDto::Threshold {
            blocks: block_threshold,
        }
    } else {
        ReceivableDto::Blocks {
            blocks: block_default,
        }
    };

    RpcDto::WalletReceivable(result)
}
