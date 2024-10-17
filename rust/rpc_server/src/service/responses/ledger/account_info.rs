use rsnano_core::Amount;
use rsnano_node::Node;
use rsnano_rpc_messages::{AccountInfoArgs, AccountInfoDto, ErrorDto, RpcDto};
use std::sync::Arc;

pub async fn account_info(node: Arc<Node>, args: AccountInfoArgs) -> RpcDto {
    let txn = node.ledger.read_txn();
    let include_confirmed = args.include_confirmed.unwrap_or(false);

    let info = match node.ledger.any().get_account(&txn, &args.account) {
        Some(account_info) => account_info,
        None => return RpcDto::Error(ErrorDto::AccountNotFound),
    };

    let confirmation_height_info = node
        .store
        .confirmation_height
        .get(&txn, &args.account)
        .unwrap();

    let mut account_info = AccountInfoDto::new(
        info.head,
        info.open_block,
        node.ledger.representative_block_hash(&txn, &info.head),
        info.balance,
        info.modified,
        info.block_count,
        info.epoch as u8,
    );

    account_info.confirmed_height = Some(confirmation_height_info.height);
    account_info.confirmation_height_frontier = Some(confirmation_height_info.frontier);

    if include_confirmed {
        let confirmed_balance = if info.block_count != confirmation_height_info.height {
            node.ledger
                .any()
                .block_balance(&txn, &confirmation_height_info.frontier)
                .unwrap_or(Amount::zero())
        } else {
            info.balance
        };
        account_info.confirmed_balance = Some(confirmed_balance);
    }

    if args.representative.unwrap_or(false) {
        account_info.representative = Some(info.representative.into());
        if include_confirmed {
            let confirmed_representative = if confirmation_height_info.height > 0 {
                if let Some(confirmed_frontier_block) = node
                    .ledger
                    .any()
                    .get_block(&txn, &confirmation_height_info.frontier)
                {
                    confirmed_frontier_block
                        .representative_field()
                        .unwrap_or_else(|| {
                            let rep_block_hash = node.ledger.representative_block_hash(
                                &txn,
                                &confirmation_height_info.frontier,
                            );
                            node.ledger
                                .any()
                                .get_block(&txn, &rep_block_hash)
                                .unwrap()
                                .representative_field()
                                .unwrap()
                        })
                } else {
                    info.representative
                }
            } else {
                info.representative
            };
            account_info.confirmed_representative = Some(confirmed_representative.into());
        }
    }

    if args.weight.unwrap_or(false) {
        account_info.weight = Some(node.ledger.weight_exact(&txn, args.account.into()));
    }

    if args.pending.unwrap_or(false) || args.receivable.unwrap_or(false) {
        let account_receivable = node.ledger.account_receivable(&txn, &args.account, false);
        account_info.pending = Some(account_receivable);
        account_info.receivable = Some(account_receivable);

        if include_confirmed {
            let confirmed_receivable = node.ledger.account_receivable(&txn, &args.account, true);
            account_info.confirmed_pending = Some(confirmed_receivable);
            account_info.confirmed_receivable = Some(confirmed_receivable);
        }
    }

    RpcDto::AccountInfo(account_info)
}
