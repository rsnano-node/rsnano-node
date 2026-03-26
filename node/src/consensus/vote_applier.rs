#[cfg(test)]
mod tests {
    use crate::consensus::{AecInsertRequest, AecService};
    use crate::{consensus::ReceivedVote, representatives::OnlineReps};
    use rsnano_ledger::RepWeightCache;
    use rsnano_nullable_clock::SteadyClock;
    use rsnano_types::{
        Amount, BlockPriority, PrivateKey, SavedBlock, UnixMillisTimestamp, Vote, VoteSource,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn update_online_weight_before_quorum_checks() {
        let block = SavedBlock::new_test_instance();
        let block_hash = block.hash();
        let rep_key = PrivateKey::from(1);
        let another_rep = PrivateKey::from(2);

        let rep_weights = Arc::new(RepWeightCache::default());
        rep_weights.put(rep_key.public_key(), Amount::nano(50_000_000));
        rep_weights.put(another_rep.public_key(), Amount::nano(65_000_000));

        let online_reps = Arc::new(Mutex::new(
            OnlineReps::builder()
                .rep_weights(rep_weights.clone())
                .finish(),
        ));
        let clock = Arc::new(SteadyClock::new_null());
        let service = AecService::new(
            Default::default(),
            std::time::Duration::from_secs(1),
            online_reps.clone(),
            clock.clone(),
            rep_weights.clone(),
            false,
        );

        online_reps
            .lock()
            .unwrap()
            .vote_observed(another_rep.public_key(), clock.now());

        assert_eq!(
            online_reps.lock().unwrap().quorum_delta(),
            Amount::nano(43_550_000)
        );

        service
            .insert_for_test(
                AecInsertRequest::new_priority(block, BlockPriority::new_test_instance()),
                clock.now(),
            )
            .unwrap();

        let vote = ReceivedVote::new(
            Vote::new(&rep_key, UnixMillisTimestamp::new(123), 0, vec![block_hash]).into(),
            VoteSource::Live,
            None,
        );

        service.apply_vote(&vote.into());

        let election = service.election_for_block(&block_hash).unwrap();
        assert_eq!(election.winner_tally(), Amount::nano(50_000_000));

        // No quorum, because the vote of our rep has to be added to the online
        // weight before the quorum is checked!
        assert_eq!(election.has_quorum(), false);
    }
}
