use crate::core::{
    Account, Amount, Block, BlockDetails, BlockHash, BlockSideband, Epoch, KeyPair, Link,
    PublicKey, RawKey, Signature, StateBlock,
};
use anyhow::Result;

pub struct StateBlockBuilder {
    account: Account,
    previous: BlockHash,
    representative: Account,
    balance: Amount,
    link: Link,
    prv_key: RawKey,
    pub_key: PublicKey,
    work: u64,
    signature: Option<Signature>,
}

impl StateBlockBuilder {
    pub fn new() -> Self {
        let key = KeyPair::new();
        Self {
            account: Account::from(1),
            previous: BlockHash::from(2),
            representative: Account::from(3),
            balance: Amount::from(4),
            link: Link::from(5),
            prv_key: key.private_key(),
            pub_key: key.public_key(),
            work: 6,
            signature: None,
        }
    }

    pub fn from(mut self, other: &StateBlock) -> Self {
        self.account = other.hashables.account;
        self.previous = other.hashables.previous;
        self.representative = other.hashables.representative;
        self.balance = other.hashables.balance;
        self.link = other.hashables.link;
        self.signature = Some(other.signature.clone());
        self.work = other.work;
        self
    }

    pub fn account(mut self, account: impl Into<Account>) -> Self {
        self.account = account.into();
        self
    }

    pub fn account_address(self, address: impl AsRef<str>) -> Result<Self> {
        Ok(self.account(Account::decode_account(address)?))
    }

    pub fn previous(mut self, previous: impl Into<BlockHash>) -> Self {
        self.previous = previous.into();
        self
    }

    pub fn previous_hex(self, previous: impl AsRef<str>) -> Result<Self> {
        Ok(self.previous(BlockHash::decode_hex(previous)?))
    }

    pub fn representative(mut self, rep: impl Into<Account>) -> Self {
        self.representative = rep.into();
        self
    }

    pub fn representative_address(self, address: impl AsRef<str>) -> Result<Self> {
        Ok(self.representative(Account::decode_account(address)?))
    }

    pub fn balance(mut self, balance: impl Into<Amount>) -> Self {
        self.balance = balance.into();
        self
    }

    pub fn balance_dec(self, balance: impl AsRef<str>) -> Result<Self> {
        Ok(self.balance(balance.as_ref().parse::<u128>()?))
    }

    pub fn link(mut self, link: impl Into<Link>) -> Self {
        self.link = link.into();
        self
    }

    pub fn link_hex(self, link: impl AsRef<str>) -> Result<Self> {
        Ok(self.link(Link::decode_hex(link)?))
    }

    pub fn sign(mut self, key: &KeyPair) -> Self {
        self.signature = None;
        self.prv_key = key.private_key();
        self.pub_key = key.public_key();
        self
    }

    pub fn signature(mut self, signature: Signature) -> Self {
        self.signature = Some(signature);
        self
    }

    pub fn sign_zero(self) -> Self {
        self.signature(Signature::new())
    }

    pub fn work(mut self, work: u64) -> Self {
        self.work = work;
        self
    }

    pub fn zero(mut self) -> Self {
        self.account = Account::zero();
        self.previous = BlockHash::zero();
        self.representative = Account::zero();
        self.balance = Amount::zero();
        self.link = Link::zero();
        self.signature = None;
        self.work = 0;
        self
    }

    pub fn build(self) -> Result<StateBlock> {
        let mut state = match self.signature {
            Some(signature) => StateBlock::with_signature(
                self.account,
                self.previous,
                self.representative,
                self.balance,
                self.link,
                signature,
                self.work,
            ),
            None => StateBlock::new(
                self.account,
                self.previous,
                self.representative,
                self.balance,
                self.link,
                &self.prv_key,
                &self.pub_key,
                self.work,
            )?,
        };

        let details = BlockDetails::new(Epoch::Epoch0, true, false, false);
        state.set_sideband(BlockSideband::new(
            self.account,
            BlockHash::zero(),
            self.balance,
            5,
            6,
            details,
            Epoch::Epoch0,
        ));

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{validate_message, BlockBuilder, StateBlock};

    #[test]
    fn state_block() {
        let block1 = BlockBuilder::state()
            .account(3)
            .previous(1)
            .representative(6)
            .balance(2)
            .link(4)
            .work(5)
            .build()
            .unwrap();

        assert_eq!(block1.hashables.account, Account::from(3));
        assert_eq!(block1.hashables.previous, BlockHash::from(1));
        assert_eq!(block1.hashables.representative, Account::from(6).into());
        assert_eq!(block1.hashables.balance, Amount::new(2));
        assert_eq!(block1.hashables.link, Link::from(4));
    }

    // original test: block_builder.from
    #[test]
    fn copy_state_block() -> Result<()> {
        let block = BlockBuilder::state()
            .account_address("xrb_15nhh1kzw3x8ohez6s75wy3jr6dqgq65oaede1fzk5hqxk4j8ehz7iqtb3to")?
            .previous_hex("FEFBCE274E75148AB31FF63EFB3082EF1126BF72BF3FA9C76A97FD5A9F0EBEC5")?
            .balance_dec("2251569974100400000000000000000000")?
            .representative_address(
                "xrb_1stofnrxuz3cai7ze75o174bpm7scwj9jn3nxsn8ntzg784jf1gzn1jjdkou",
            )?
            .link_hex("E16DD58C1EFA8B521545B0A74375AA994D9FC43828A4266D75ECF57F07A7EE86")?
            .build()?;

        assert_eq!(
            block.hash().to_string(),
            "2D243F8F92CDD0AD94A1D456A6B15F3BE7A6FCBD98D4C5831D06D15C818CD81F"
        );

        let block2 = BlockBuilder::state().from(&block).build()?;
        assert_eq!(
            block2.hash().to_string(),
            "2D243F8F92CDD0AD94A1D456A6B15F3BE7A6FCBD98D4C5831D06D15C818CD81F"
        );

        let block3 = BlockBuilder::state()
            .from(&block)
            .sign_zero()
            .work(0)
            .build()?;
        assert_eq!(
            block3.hash().to_string(),
            "2D243F8F92CDD0AD94A1D456A6B15F3BE7A6FCBD98D4C5831D06D15C818CD81F"
        );

        Ok(())
    }

    // original test: block_builder.zeroed_state_block
    #[test]
    fn zeroed_state_block() -> Result<()> {
        let key = KeyPair::new();
        // Make sure manually- and builder constructed all-zero blocks have equal hashes, and check signature.
        let zero_block_manual = BlockBuilder::state()
            .account(0)
            .previous(0)
            .representative(0)
            .balance(0)
            .link(0)
            .sign(&key)
            .work(0)
            .build()?;

        let zero_block_build = BlockBuilder::state().zero().sign(&key).build()?;
        assert_eq!(zero_block_manual.hash(), zero_block_build.hash());
        validate_message(
            &key.public_key(),
            zero_block_build.hash().as_bytes(),
            &zero_block_build.signature,
        )?;
        Ok(())
    }

    // original test: block_builder.state
    #[test]
    fn state_block_from_live_network() -> Result<()> {
        // Test against a random hash from the live network
        let block = BlockBuilder::state()
            .account_address("xrb_15nhh1kzw3x8ohez6s75wy3jr6dqgq65oaede1fzk5hqxk4j8ehz7iqtb3to")?
            .previous_hex("FEFBCE274E75148AB31FF63EFB3082EF1126BF72BF3FA9C76A97FD5A9F0EBEC5")?
            .balance_dec("2251569974100400000000000000000000")?
            .representative_address(
                "xrb_1stofnrxuz3cai7ze75o174bpm7scwj9jn3nxsn8ntzg784jf1gzn1jjdkou",
            )?
            .link_hex("E16DD58C1EFA8B521545B0A74375AA994D9FC43828A4266D75ECF57F07A7EE86")?
            .build()?;
        assert_eq!(
            block.hash().to_string(),
            "2D243F8F92CDD0AD94A1D456A6B15F3BE7A6FCBD98D4C5831D06D15C818CD81F"
        );
        assert!(block.source().is_zero());
        assert!(block.destination().is_zero());
        assert_eq!(
            block.link().encode_hex(),
            "E16DD58C1EFA8B521545B0A74375AA994D9FC43828A4266D75ECF57F07A7EE86"
        );
        Ok(())
    }

    // original test: block_builder.state_equality
    #[test]
    fn state_equality() {
        let key1 = KeyPair::new();
        let key2 = KeyPair::new();
        let block1 = StateBlock::new(
            Account::from(key1.public_key()),
            BlockHash::from(1),
            Account::from(key2.public_key()),
            Amount::new(2),
            Link::from(4),
            &key1.private_key(),
            &key1.public_key(),
            5,
        )
        .unwrap();

        let block2 = BlockBuilder::state()
            .account(key1.public_key())
            .previous(1)
            .representative(key2.public_key())
            .balance(2)
            .link(4)
            .sign(&key1)
            .work(5)
            .build()
            .unwrap();

        assert_eq!(block1.hash(), block2.hash());
        assert_eq!(block1.work, block2.work);
    }
}
