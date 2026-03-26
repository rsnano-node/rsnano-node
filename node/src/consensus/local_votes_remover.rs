use super::{AecService, LocalVoteHistory};
use rsnano_types::{BlockHash, QualifiedRoot};
use std::sync::Arc;

pub(crate) struct LocalVotesRemover {
    pub(crate) vote_history: Arc<LocalVoteHistory>,
    pub(crate) aec_service: Arc<AecService>,
}

impl LocalVotesRemover {
    /// Removes votes that were created by this node from an election
    /// if the election winner has changed
    pub fn remove_local_votes(&self, previous_winner: &BlockHash, root: &QualifiedRoot) {
        let votes = self.vote_history.votes(&root.root, previous_winner, false);

        self.aec_service
            .remove_votes(root, votes.iter().map(|i| i.voter));

        self.vote_history.erase(&root.root);
    }
}
