use crate::command_handler::RpcCommandHandler;
use anyhow::anyhow;
use indexmap::IndexMap;
use rsnano_rpc_messages::{
    ConfirmationBlockInfoDto, ConfirmationInfoArgs, ConfirmationInfoResponse,
};
use rsnano_types::{Account, Amount};

impl RpcCommandHandler {
    pub(crate) fn confirmation_info(
        &self,
        args: ConfirmationInfoArgs,
    ) -> anyhow::Result<ConfirmationInfoResponse> {
        let include_representatives = args.representatives.unwrap_or(false.into()).inner();
        let contents = args.contents.unwrap_or(true.into()).inner();
        let election = self
            .node
            .active
            .election_for_root(&args.root)
            .ok_or_else(|| anyhow!("Active confirmation not found"))?;

        let announcements = 0; // not supported in RsNano
        let voters = election.votes().len();
        let last_winner = election.winner().hash();
        let final_tally = election.winner_final_tally();
        let mut total_tally = Amount::ZERO;
        let mut blocks = IndexMap::new();

        for block in election.candidate_blocks().values() {
            let tally = election.tallies().get(&block.hash());

            total_tally += tally;

            let contents = if contents {
                Some(block.json_representation())
            } else {
                None
            };

            let (representatives, representatives_final) = if include_representatives {
                let mut reps = IndexMap::new();
                let mut reps_final = IndexMap::new();
                for (representative, vote) in election.votes() {
                    if block.hash() == vote.hash {
                        let amount = self.node.ledger.rep_weights.weight(representative);

                        reps.insert(Account::from(representative), amount);

                        if vote.is_final_vote() {
                            reps_final.insert(Account::from(representative), amount);
                        }
                    }
                }
                reps.sort_by(|k1, _, k2, _| k2.cmp(k1));
                reps_final.sort_by(|k1, _, k2, _| k2.cmp(k1));
                (Some(reps), Some(reps_final))
            } else {
                (None, None)
            };

            let entry = ConfirmationBlockInfoDto {
                tally,
                contents,
                representatives,
                representatives_final,
            };

            blocks.insert(block.hash(), entry);
        }

        Ok(ConfirmationInfoResponse {
            announcements: (announcements as u32).into(),
            voters: (voters as u32).into(),
            last_winner,
            total_tally,
            final_tally,
            blocks,
        })
    }
}
