use rsnano_core::Account;
use rsnano_node::Node;
use rsnano_rpc_messages::{RepresentativesOnlineArgs, RepresentativesOnlineDto, RpcDto};
use std::{collections::HashMap, sync::Arc};

pub async fn representatives_online(node: Arc<Node>, args: RepresentativesOnlineArgs) -> RpcDto {
    let lock = node.online_reps.lock().unwrap();
    let online_reps = lock.online_reps();
    let weight = args.weight.unwrap_or(false);

    let mut representatives = HashMap::new();

    let accounts_to_filter = args.accounts.unwrap_or_default();
    let filtering = !accounts_to_filter.is_empty();

    for pk in online_reps {
        let account = Account::from(pk.clone());

        if filtering && !accounts_to_filter.contains(&account) {
            continue;
        }

        let account_weight = if weight {
            Some(node.ledger.weight(&pk))
        } else {
            None
        };

        representatives.insert(account, account_weight);
    }

    let dto = RepresentativesOnlineDto { representatives };

    RpcDto::RepresentativesOnline(dto)
}
