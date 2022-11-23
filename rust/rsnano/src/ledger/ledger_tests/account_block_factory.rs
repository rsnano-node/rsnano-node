use crate::{
    core::{
        Account, AccountInfo, Amount, BlockBuilder, BlockHash, ChangeBlockBuilder, Epoch, KeyPair,
        OpenBlockBuilder, ReceiveBlockBuilder, StateBlockBuilder,
    },
    ledger::{datastore::Transaction, Ledger, DEV_GENESIS_KEY},
    DEV_CONSTANTS,
};

/// Test helper that creates blocks for a single account
pub(crate) struct AccountBlockFactory<'a> {
    pub key: KeyPair,
    ledger: &'a Ledger,
}

impl<'a> AccountBlockFactory<'a> {
    pub(crate) fn new(ledger: &'a Ledger) -> Self {
        Self {
            key: KeyPair::new(),
            ledger,
        }
    }

    pub(crate) fn genesis(ledger: &'a Ledger) -> Self {
        Self {
            key: DEV_GENESIS_KEY.clone(),
            ledger,
        }
    }

    pub(crate) fn account(&self) -> Account {
        self.key.public_key().into()
    }

    pub(crate) fn info(&self, txn: &dyn Transaction) -> Option<AccountInfo> {
        self.ledger.store.account().get(txn, &self.account())
    }

    pub(crate) fn open(&self, source: BlockHash) -> OpenBlockBuilder {
        BlockBuilder::open()
            .source(source)
            .representative(self.account())
            .account(self.account())
            .sign(&self.key)
    }

    pub(crate) fn epoch_v1(&self, txn: &dyn Transaction) -> StateBlockBuilder {
        let info = self.info(txn).unwrap();
        BlockBuilder::state()
            .account(self.account())
            .previous(info.head)
            .representative(info.representative)
            .balance(info.balance)
            .link(*DEV_CONSTANTS.epochs.link(Epoch::Epoch1).unwrap())
            .sign(&DEV_GENESIS_KEY)
    }

    pub(crate) fn epoch_v1_open(&self) -> StateBlockBuilder {
        BlockBuilder::state()
            .account(self.account())
            .previous(0)
            .representative(0)
            .balance(0)
            .link(self.ledger.epoch_link(Epoch::Epoch1).unwrap())
            .sign(&DEV_GENESIS_KEY)
    }

    pub(crate) fn epoch_v2(&self, txn: &dyn Transaction) -> StateBlockBuilder {
        let info = self.info(txn).unwrap();
        BlockBuilder::state()
            .account(self.account())
            .previous(info.head)
            .representative(info.representative)
            .balance(info.balance)
            .link(*DEV_CONSTANTS.epochs.link(Epoch::Epoch2).unwrap())
            .sign(&DEV_GENESIS_KEY)
    }

    pub(crate) fn epoch_v2_open(&self) -> StateBlockBuilder {
        BlockBuilder::state()
            .account(self.account())
            .previous(0)
            .representative(0)
            .balance(0)
            .link(*DEV_CONSTANTS.epochs.link(Epoch::Epoch2).unwrap())
            .sign(&DEV_GENESIS_KEY)
    }

    pub(crate) fn change_representative(
        &self,
        txn: &dyn Transaction,
        representative: Account,
    ) -> ChangeBlockBuilder {
        let info = self.info(txn).unwrap();
        BlockBuilder::change()
            .previous(info.head)
            .representative(representative)
            .sign(&self.key)
    }

    pub(crate) fn receive(
        &self,
        txn: &dyn Transaction,
        send_hash: BlockHash,
    ) -> ReceiveBlockBuilder {
        let receiver_info = self.info(txn).unwrap();
        BlockBuilder::receive()
            .previous(receiver_info.head)
            .source(send_hash)
            .sign(&self.key)
    }

    pub(crate) fn state_send(
        &self,
        txn: &dyn Transaction,
        destination: Account,
        amount: Amount,
    ) -> StateBlockBuilder {
        let info = self.info(txn).unwrap();
        BlockBuilder::state()
            .account(self.account())
            .previous(info.head)
            .representative(info.representative)
            .balance(info.balance - amount)
            .link(destination)
            .sign(&self.key)
    }

    pub(crate) fn state_receive(
        &self,
        txn: &dyn Transaction,
        send_hash: BlockHash,
    ) -> StateBlockBuilder {
        let receiver_info = self.info(txn).unwrap();
        let amount_sent = self.ledger.amount(txn, &send_hash).unwrap();
        BlockBuilder::state()
            .account(self.account())
            .previous(receiver_info.head)
            .representative(receiver_info.representative)
            .balance(receiver_info.balance + amount_sent)
            .link(send_hash)
            .sign(&self.key)
    }
}
