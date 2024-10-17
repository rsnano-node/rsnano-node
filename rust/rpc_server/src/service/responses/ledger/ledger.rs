use rsnano_core::{Account, AccountInfo, Amount};
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, LedgerAccountInfo, LedgerArgs, LedgerDto, RpcDto};
use std::{collections::HashMap, sync::Arc, u64};

pub async fn ledger(node: Arc<Node>, enable_control: bool, args: LedgerArgs) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let account = args.account;
    let count = args.count.unwrap_or(u64::MAX);
    let representative = args.representative.unwrap_or(false);
    let weight = args.weight.unwrap_or(false);
    let pending = args.pending.unwrap_or(false);
    let receivable = args.receivable.unwrap_or(pending);
    let modified_since = args.modified_since.unwrap_or(0);
    let sorting = args.sorting.unwrap_or(false);
    let threshold = args.threshold.unwrap_or(Amount::zero());

    let mut accounts_json: HashMap<Account, LedgerAccountInfo> = HashMap::new();
    let block_transaction = node.store.tx_begin_read();

    let account_iter: Box<dyn Iterator<Item = (Account, AccountInfo)>> = match account {
        Some(acc) => Box::new(node.store.account.iter_range(&block_transaction, acc..)),
        None => Box::new(node.store.account.iter(&block_transaction)),
    };

    if !sorting {
        for (current_account, info) in account_iter {
            if info.modified >= modified_since {
                let account_receivable = if receivable {
                    node.ledger
                        .account_receivable(&block_transaction, &current_account, false)
                } else {
                    Amount::zero()
                };
                let total_balance = info.balance + account_receivable;
                if total_balance >= threshold {
                    process_account(
                        node.clone(),
                        current_account,
                        &info,
                        representative,
                        weight,
                        receivable,
                        &mut accounts_json,
                        account_receivable,
                    );
                }
            }
            if accounts_json.len() >= count as usize {
                break;
            }
        }
    } else {
        let mut ledger_l: Vec<(Amount, Account)> = Vec::new();
        match account {
            Some(acc) => {
                let mut iter = node.store.account.begin_account(&block_transaction, &acc);
                while let Some((current_account, info)) = iter.current() {
                    if info.modified >= modified_since {
                        ledger_l.push((info.balance, *current_account));
                    }
                    iter.next();
                }
            }
            None => {
                let iter = node.store.account.iter(&block_transaction);
                for (account, info) in iter {
                    if info.modified >= modified_since {
                        ledger_l.push((info.balance, account));
                    }
                }
            }
        }

        ledger_l.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, account) in ledger_l {
            if let Some(info) = node.store.account.get(&block_transaction, &account) {
                let account_receivable = if receivable {
                    node.ledger
                        .account_receivable(&block_transaction, &account, false)
                } else {
                    Amount::zero()
                };
                let total_balance = info.balance + account_receivable;
                if total_balance >= threshold {
                    process_account(
                        node.clone(),
                        account,
                        &info,
                        representative,
                        weight,
                        receivable,
                        &mut accounts_json,
                        account_receivable,
                    );
                    if accounts_json.len() >= count as usize {
                        break;
                    }
                }
            }
        }
    }

    RpcDto::Ledger(LedgerDto {
        accounts: accounts_json,
    })
}

fn process_account(
    node: Arc<Node>,
    account: Account,
    info: &AccountInfo,
    representative: bool,
    weight: bool,
    pending: bool,
    accounts_json: &mut HashMap<Account, LedgerAccountInfo>,
    account_receivable: Amount,
) {
    let block_transaction = node.ledger.read_txn();
    let mut representative_opt = None;
    let mut weight_opt = None;
    let mut pending_opt = None;

    if representative {
        representative_opt = Some(info.representative);
    }
    if weight {
        weight_opt = Some(node.ledger.weight(&account.into()));
    }
    if pending {
        pending_opt = Some(account_receivable);
    }

    let entry = LedgerAccountInfo::new(
        info.head,
        info.open_block,
        node.ledger
            .representative_block_hash(&block_transaction, &info.head),
        info.balance,
        info.modified,
        info.block_count,
        representative_opt.map(|inner| inner.into()),
        weight_opt,
        pending_opt, // Pending field
        pending_opt, // Receivable field
    );
    accounts_json.insert(account, entry);
}
