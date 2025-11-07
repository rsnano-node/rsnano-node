use crate::{Aggregatable, MessageVariant, ProposalHash};
use bitvec::prelude::BitArray;
#[cfg(feature = "ledger_snapshots")]
use rsnano_types::SnapshotNumber;
use rsnano_types::{
    Blake2Hash, Blake2HashBuilder, DeserializationError, PrivateKey, PublicKey, Signature,
    read_u32_be,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalVote {
    pub snapshot_number: SnapshotNumber,
    pub voter: PublicKey,
    pub signature: Signature,
    pub proposal_hash: ProposalHash,
}

impl ProposalVote {
    pub fn new(
        proposal_hash: ProposalHash,
        private_key: &PrivateKey,
        snapshot_number: SnapshotNumber,
    ) -> Self {
        let mut proposal_vote = Self {
            snapshot_number,
            proposal_hash,
            voter: private_key.public_key(),
            signature: Signature::default(),
        };

        proposal_vote.signature = private_key.sign(proposal_hash.as_bytes());

        proposal_vote
    }

    pub fn new_test_instance() -> Self {
        Self {
            snapshot_number: 1,
            proposal_hash: ProposalHash::from(1),
            voter: PublicKey::from(2),
            signature: Signature::from_bytes([1; 64]),
        }
    }

    pub fn serialize<T>(&self, writer: &mut T) -> std::io::Result<()>
    where
        T: std::io::Write,
    {
        writer.write_all(&self.snapshot_number.to_be_bytes())?;
        self.voter.serialize(writer)?;
        self.signature.serialize(writer)?;
        self.proposal_hash.serialize(writer)
    }

    pub(crate) fn serialized_size(extensions: BitArray<u16>) -> usize {
        extensions.data as usize
    }

    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, DeserializationError> {
        let snapshot_number = read_u32_be(&mut bytes)?;
        let voter = PublicKey::deserialize(&mut bytes)?;
        let signature = Signature::deserialize(&mut bytes)?;
        let proposal_hash = ProposalHash::deserialize(&mut bytes)?;
        Ok(Self {
            snapshot_number,
            voter,
            signature,
            proposal_hash,
        })
    }
}

impl MessageVariant for ProposalVote {
    fn header_extensions(&self, payload_len: u16) -> BitArray<u16> {
        BitArray::new(payload_len)
    }
}

impl Aggregatable for ProposalVote {
    fn signer(&self) -> PublicKey {
        self.voter
    }

    fn hash(&self) -> Blake2Hash {
        let proposal_hash = self.proposal_hash;
        let mut hash_builder: Blake2HashBuilder = Blake2HashBuilder::default();
        hash_builder = hash_builder
            .update(self.snapshot_number.to_be_bytes())
            .update(proposal_hash.as_bytes())
            .update(self.voter.as_bytes());
        hash_builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aggregatable, Message, Proposal, ProposalVote, assert_deserializable};

    #[test]
    fn sign_new_proposal_vote() {
        let private_key = PrivateKey::from(42);
        let proposal = Proposal::new_test_instance();

        let proposal_vote = ProposalVote::new(proposal.hash(), &private_key, 0);

        assert_eq!(proposal_vote.voter, private_key.public_key());
        assert_eq!(proposal_vote.proposal_hash, proposal.hash());

        let result = proposal_vote.voter.verify(
            proposal_vote.proposal_hash.as_bytes(),
            &proposal_vote.signature,
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn proposal_vote_message_is_serializable() {
        let message = Message::SnapshotProposalVote(ProposalVote::new_test_instance());
        assert_deserializable(&message);
    }

    #[test]
    fn proposal_vote_hash() {
        let proposal_hash = ProposalHash::from(1);
        let proposal_vote1 = ProposalVote::new(proposal_hash, &PrivateKey::from(1), 0);
        let proposal_vote2 = ProposalVote::new(proposal_hash, &PrivateKey::from(2), 0);
        let proposal_vote3 = ProposalVote::new(proposal_hash, &PrivateKey::from(2), 1);
        let proposal_vote4 = ProposalVote::new(ProposalHash::from(2), &PrivateKey::from(1), 0);

        assert_ne!(proposal_vote1.hash(), proposal_vote2.hash());
        assert_ne!(proposal_vote3.hash(), proposal_vote2.hash());
        assert_ne!(proposal_vote1.hash(), proposal_vote4.hash());
    }
}
