use crate::{Aggregatable, MessageVariant, Preproposal, PreproposalHash};
use bitvec::array::BitArray;
use rsnano_types::{
    Blake2Hash, Blake2HashBuilder, DeserializationError, PrivateKey, PublicKey, Signature,
};

pub type PreproposalsHash = Blake2Hash;
pub type ProposalHash = Blake2Hash;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub preproposal_hashes: Vec<PreproposalHash>,
    pub signer: PublicKey,
    pub signature: Signature,
}

impl Proposal {
    pub fn new<'a>(
        preproposals: impl IntoIterator<Item = &'a Preproposal>,
        private_key: &PrivateKey,
    ) -> Self {
        let preproposals: Vec<PreproposalHash> =
            preproposals.into_iter().map(|p| p.hash()).collect();

        let mut proposal = Self {
            preproposal_hashes: preproposals,
            signer: private_key.public_key(),
            signature: Signature::default(),
        };

        proposal.signature = private_key.sign(proposal.hash().as_bytes());

        proposal
    }

    pub fn new_test_instance() -> Self {
        Self {
            preproposal_hashes: vec![PreproposalHash::from(1)],
            signer: PublicKey::from(2),
            signature: Signature::from_bytes([1; 64]),
        }
    }

    fn preproposals_hash(&self) -> PreproposalsHash {
        Proposal::hash_preproposals(&self.preproposal_hashes)
    }

    fn hash_preproposals(preproposals_hashes: &[PreproposalHash]) -> PreproposalsHash {
        let mut unique_sorted: Vec<PreproposalHash> = preproposals_hashes.to_vec();
        unique_sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let mut hash_builder = Blake2HashBuilder::default();
        for hash in unique_sorted.iter() {
            hash_builder = hash_builder.update(hash.as_bytes());
        }
        hash_builder.build()
    }

    pub fn serialize<T>(&self, writer: &mut T) -> std::io::Result<()>
    where
        T: std::io::Write,
    {
        self.signer.serialize(writer)?;
        self.signature.serialize(writer)?;
        for preproposal in &self.preproposal_hashes {
            preproposal.serialize(writer)?;
        }
        Ok(())
    }

    pub const fn serialized_size(extensions: BitArray<u16>) -> usize {
        extensions.data as usize
    }

    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, DeserializationError> {
        let signer = PublicKey::deserialize(&mut bytes)?;
        let signature = Signature::deserialize(&mut bytes)?;
        let mut preproposals = Vec::new();

        while !bytes.is_empty() {
            let preproposal = PreproposalHash::deserialize(&mut bytes)?;
            preproposals.push(preproposal);
        }

        Ok(Proposal {
            preproposal_hashes: preproposals,
            signer,
            signature,
        })
    }
}

impl MessageVariant for Proposal {
    fn header_extensions(&self, payload_len: u16) -> BitArray<u16> {
        BitArray::new(payload_len)
    }
}

impl Aggregatable for Proposal {
    fn signer(&self) -> PublicKey {
        self.signer
    }

    fn hash(&self) -> Blake2Hash {
        let preproposals_hash = self.preproposals_hash();
        let mut hash_builder: Blake2HashBuilder = Blake2HashBuilder::default();
        hash_builder = hash_builder
            .update(preproposals_hash.as_bytes())
            .update(self.signer.as_bytes());
        hash_builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Preproposal, assert_deserializable};
    use rsnano_types::{Account, BlockHash};

    #[test]
    fn hash_preproposals_is_order_invariant() {
        let pre1 = Preproposal::new(
            vec![
                (Account::from(1), BlockHash::from(10)),
                (Account::from(2), BlockHash::from(20)),
            ],
            &PrivateKey::new(),
            0
        );
        let pre2 = Preproposal::new(
            vec![
                (Account::from(2), BlockHash::from(20)),
                (Account::from(1), BlockHash::from(10)),
            ],
            &PrivateKey::new(),
            0
        );

        let p1 = Proposal::new(&[pre1.clone(), pre2.clone()], &PrivateKey::new());
        let p2 = Proposal::new(&[pre2, pre1], &PrivateKey::new());

        assert_eq!(p1.preproposals_hash(), p2.preproposals_hash());
    }

    #[test]
    fn sign_new_proposal() {
        let private_key = PrivateKey::from(42);
        let preproposals = [Preproposal::new_test_instance()];

        let proposal = Proposal::new(&preproposals, &private_key);

        assert_eq!(proposal.signer, private_key.public_key());

        let result = proposal
            .signer
            .verify(proposal.hash().as_bytes(), &proposal.signature);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn proposal_serialization() {
        let message = Message::SnapshotProposal(Proposal::new_test_instance());
        assert_deserializable(&message);
    }
}
