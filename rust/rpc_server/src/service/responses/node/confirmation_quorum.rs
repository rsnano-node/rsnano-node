use rsnano_core::utils::NULL_ENDPOINT;
use rsnano_node::Node;
use rsnano_rpc_messages::{ConfirmationQuorumArgs, ConfirmationQuorumDto, PeerDetailsDto, RpcDto};
use std::sync::Arc;

pub async fn confirmation_quorum(node: Arc<Node>, args: ConfirmationQuorumArgs) -> RpcDto {
    let quorum = node.online_reps.lock().unwrap();

    let mut confirmation_quorum_dto = ConfirmationQuorumDto {
        quorum_delta: quorum.quorum_delta(),
        online_weight_quorum_percent: quorum.quorum_percent(),
        online_weight_minimum: quorum.online_weight_minimum(),
        online_stake_total: quorum.online_weight(),
        trended_stake_total: quorum.trended_weight(),
        peers_stake_total: quorum.peered_weight(),
        peers: None,
    };

    if args.peer_details.unwrap_or(false) {
        let peers = quorum
            .peered_reps()
            .iter()
            .map(|rep| {
                let endpoint = node
                    .network_info
                    .read()
                    .unwrap()
                    .get(rep.channel_id)
                    .map(|c| c.peer_addr())
                    .unwrap_or(NULL_ENDPOINT);

                PeerDetailsDto {
                    account: rep.account.into(),
                    ip: endpoint,
                    weight: node.ledger.weight(&rep.account),
                }
            })
            .collect();

        confirmation_quorum_dto.peers = Some(peers);
    }

    RpcDto::ConfirmationQuorum(confirmation_quorum_dto)
}
