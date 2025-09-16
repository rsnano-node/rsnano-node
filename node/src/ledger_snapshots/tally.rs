use rsnano_messages::Aggregatable;
use rsnano_types::{Amount, Blake2Hash, PublicKey};
use std::collections::{HashMap, HashSet};

use crate::representatives::ConsensusParams;

pub(super) struct Aggregator<T: Aggregatable> {
    values: HashMap<Blake2Hash, T>,
    signers: HashSet<PublicKey>,
}

impl<T: Aggregatable> Default for Aggregator<T> {
    fn default() -> Self {
        Self {
            values: Default::default(),
            signers: Default::default(),
        }
    }
}

impl<T: Aggregatable> Aggregator<T> {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn contains(&self, hash: &Blake2Hash) -> bool {
        self.values.contains_key(hash)
    }

    pub fn add(&mut self, value: T) {
        if self.signers.insert(value.signer()) {
            self.values.insert(value.hash(), value);
        }
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.values.values()
    }

    /// Quorum is achieved when the combined vote weight of all received valid values reaches at least 67%
    pub(crate) fn has_quorum(&self, consensus_params: &ConsensusParams) -> bool {
        let mut weight = Amount::ZERO;
        for (_, value) in &self.values {
            weight += consensus_params.rep_weights.weight(&value.signer());
        }
        weight >= consensus_params.quorum_weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_ledger::RepWeights;
    use rsnano_messages::{Preproposal, PreproposalHash};
    use rsnano_types::{Account, BlockHash, PrivateKey};

    #[test]
    fn a_new_aggregator_is_empty() {
        let aggregator = Aggregator::<Preproposal>::default();
        assert_eq!(aggregator.len(), 0);
        assert!(aggregator.is_empty());
        assert_eq!(aggregator.contains(&PreproposalHash::from(123)), false);
    }

    #[test]
    fn add_preproposal() {
        let mut aggregator = Aggregator::default();

        let preproposal = Preproposal::new_test_instance();
        aggregator.add(preproposal.clone());

        assert_eq!(aggregator.len(), 1);
        assert_eq!(aggregator.is_empty(), false);
        assert_eq!(aggregator.contains(&PreproposalHash::from(123)), false);
        assert_eq!(aggregator.contains(&preproposal.hash()), true);
    }

    #[test]
    fn only_allow_one_preproposal_per_signer() {
        let rep_key = PrivateKey::from(1);
        let mut aggregator = Aggregator::default();

        let preproposal1 =
            Preproposal::new(vec![(Account::from(1), BlockHash::from(10))], &rep_key);
        aggregator.add(preproposal1.clone());

        let preproposal2 =
            Preproposal::new(vec![(Account::from(2), BlockHash::from(20))], &rep_key);
        aggregator.add(preproposal2.clone());

        assert_eq!(aggregator.len(), 1, "Should only contain one preproposal");
        assert!(
            aggregator.contains(&preproposal1.hash()),
            "Should contain preproposal1"
        );
    }

    #[test]
    fn no_quorum_if_value_does_not_have_enough_vote_weight() {
        let rep_key = PrivateKey::from(1);
        let weight = Amount::nano(10_000);
        let mut rep_weights = RepWeights::new();
        rep_weights.insert(rep_key.public_key(), weight);
        let consensus_params = ConsensusParams { rep_weights, quorum_weight: Amount::MAX };

        let mut aggregator = Aggregator::default();
        aggregator.add(Preproposal::new(Vec::new(), &rep_key));

        assert_eq!(
            aggregator.has_quorum(&consensus_params),
            false
        );
    }

    #[test]
    fn reach_quorum() {
        let rep_key1 = PrivateKey::from(1);
        let rep_key2 = PrivateKey::from(2);
        let weight1 = Amount::nano(100_000);
        let weight2 = Amount::nano(200_000);
        let quorum_weight = weight1 + weight2;

        let mut rep_weights = RepWeights::new();
        rep_weights.insert(rep_key1.public_key(), weight1);
        rep_weights.insert(rep_key2.public_key(), weight2);

        let mut aggregator = Aggregator::default();
        let consensus_params = ConsensusParams { rep_weights, quorum_weight };

        let preproposal1 = Preproposal::new(test_frontiers(), &rep_key1);
        aggregator.add(preproposal1.clone());
        let preproposal2 = Preproposal::new(test_frontiers(), &rep_key2);
        aggregator.add(preproposal2.clone());

        assert_eq!(
            aggregator.has_quorum(&consensus_params),
            true
        );
    }

    fn test_frontiers() -> Vec<(Account, BlockHash)> {
        vec![(Account::from(1), BlockHash::from(10))]
    }
}
