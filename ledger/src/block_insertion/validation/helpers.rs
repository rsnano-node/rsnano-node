use rsnano_types::{
    AccountInfo, Amount, Block, BlockBase, BlockDetails, BlockHash, BlockSideband, Epoch, Epochs,
    PendingInfo, PendingKey, PublicKey, StateBlock,
};

use super::BlockValidator;
use std::cmp::Ordering;

impl<'a> BlockValidator<'a> {
    pub(crate) fn account_exists(&self) -> bool {
        self.old_account_info.is_some()
    }

    pub(crate) fn is_new_account(&self) -> bool {
        self.old_account_info.is_none()
    }

    pub(crate) fn previous_balance(&self) -> Amount {
        self.previous_block
            .as_ref()
            .map(|b| b.balance())
            .unwrap_or_default()
    }

    pub(crate) fn is_send(&self) -> bool {
        match self.block {
            Block::LegacySend(_) => true,
            Block::State(state) => match &self.old_account_info {
                Some(info) => state.balance() < info.balance,
                None => false,
            },
            _ => false,
        }
    }

    pub(crate) fn is_receive(&self) -> bool {
        let old_balance = self
            .old_account_info
            .as_ref()
            .map(|i| i.balance)
            .unwrap_or_default();
        is_receive_block(self.block, &old_balance, self.epochs)
    }

    pub(crate) fn source_epoch(&self) -> Epoch {
        self.pending_receive_info
            .as_ref()
            .map(|p| p.epoch)
            .unwrap_or(Epoch::Epoch0)
    }

    pub(crate) fn amount_received(&self) -> Amount {
        match &self.block {
            Block::LegacyReceive(_) | Block::LegacyOpen(_) => self.pending_amount(),
            Block::State(state) => {
                let previous = self.previous_balance();
                if previous < state.balance() {
                    state.balance() - previous
                } else {
                    Amount::ZERO
                }
            }
            _ => Amount::ZERO,
        }
    }

    pub fn pending_amount(&self) -> Amount {
        self.pending_receive_info
            .as_ref()
            .map(|i| i.amount)
            .unwrap_or_default()
    }

    pub(crate) fn amount_sent(&self) -> Amount {
        if let Some(info) = &self.old_account_info {
            let balance = match self.block {
                Block::LegacySend(i) => Some(i.balance()),
                Block::State(i) => Some(i.balance()),
                _ => None,
            };
            if let Some(balance) = balance
                && balance < info.balance
            {
                return info.balance - balance;
            }
        }
        Amount::ZERO
    }

    pub(crate) fn new_balance(&self) -> Amount {
        self.old_balance() + self.amount_received() - self.amount_sent()
    }

    fn old_balance(&self) -> Amount {
        self.old_account_info
            .as_ref()
            .map(|i| i.balance)
            .unwrap_or_default()
    }

    pub(crate) fn has_epoch_link(&self, state_block: &StateBlock) -> bool {
        self.epochs.is_epoch_link(&state_block.link())
    }

    /// This check only makes sense after ensure_previous_block_exists_for_epoch_block_candidate,
    /// because we need the previous block for the balance change check!
    pub(crate) fn is_epoch_block(&self) -> bool {
        match self.block {
            Block::State(state_block) => {
                self.has_epoch_link(state_block) && self.previous_balance() == state_block.balance()
            }
            _ => false,
        }
    }

    pub(crate) fn block_epoch_version(&self) -> Epoch {
        match self.block {
            Block::State(state) => self.epochs.epoch(&state.link()).unwrap_or(Epoch::Invalid),
            _ => Epoch::Epoch0,
        }
    }

    pub(crate) fn epoch(&self) -> Epoch {
        if self.is_epoch_block() {
            self.block_epoch_version()
        } else {
            std::cmp::max(self.old_epoch_version(), self.source_epoch())
        }
    }

    fn old_epoch_version(&self) -> Epoch {
        self.old_account_info
            .as_ref()
            .map(|i| i.epoch)
            .unwrap_or(Epoch::Epoch0)
    }

    pub(crate) fn open_block(&self) -> BlockHash {
        match &self.old_account_info {
            Some(info) => info.open_block,
            None => self.block.hash(),
        }
    }

    pub(crate) fn new_representative(&self) -> PublicKey {
        self.block
            .representative_field()
            .unwrap_or(self.old_representative())
    }

    fn old_representative(&self) -> PublicKey {
        self.old_account_info
            .as_ref()
            .map(|x| x.representative)
            .unwrap_or_default()
    }

    pub(crate) fn amount(&self) -> Amount {
        let old_balance = self.old_balance();
        let new_balance = self.new_balance();

        if old_balance > new_balance {
            old_balance - new_balance
        } else {
            new_balance - old_balance
        }
    }

    pub(crate) fn delete_received_pending_info(&self) -> Option<PendingKey> {
        if self.is_receive() {
            Some(PendingKey::new(self.account, self.block.source_or_link()))
        } else {
            None
        }
    }

    pub(crate) fn new_pending_info(&self) -> Option<(PendingKey, PendingInfo)> {
        match self.block {
            Block::State(_) => {
                if self.is_send() {
                    let key = PendingKey::for_send_block(self.block);
                    let info = PendingInfo::new(self.account, self.amount(), self.epoch());
                    Some((key, info))
                } else {
                    None
                }
            }
            Block::LegacySend(send) => {
                let amount_sent = self.amount_sent();
                Some((
                    PendingKey::new(send.destination(), send.hash()),
                    PendingInfo::new(self.account, amount_sent, Epoch::Epoch0),
                ))
            }
            _ => None,
        }
    }

    pub(crate) fn new_sideband(&self) -> BlockSideband {
        BlockSideband {
            height: self.new_block_count(),
            timestamp: self.now,
            account: self.account,
            balance: self.new_balance(),
            details: self.block_details(),
            source_epoch: self.source_epoch(),
        }
    }

    pub(crate) fn new_account_info(&self) -> AccountInfo {
        AccountInfo {
            head: self.block.hash(),
            representative: self.new_representative(),
            open_block: self.open_block(),
            balance: self.new_balance(),
            modified: self.now.into(),
            block_count: self.new_block_count(),
            epoch: self.epoch(),
        }
    }

    pub(crate) fn new_block_count(&self) -> u64 {
        self.old_account_info
            .as_ref()
            .map(|info| info.block_count)
            .unwrap_or_default()
            + 1
    }

    pub(crate) fn block_details(&self) -> BlockDetails {
        BlockDetails::new(
            self.epoch(),
            self.is_send(),
            self.is_receive(),
            self.is_epoch_block(),
        )
    }
}

fn is_receive_block(block: &Block, old_balance: &Amount, epochs: &Epochs) -> bool {
    match block {
        Block::LegacyReceive(_) | Block::LegacyOpen(_) => true,
        Block::LegacyChange(_) | Block::LegacySend(_) => false,
        Block::State(state_block) => {
            match state_block.balance().cmp(old_balance) {
                Ordering::Less => false,   // send block
                Ordering::Greater => true, // receive block
                Ordering::Equal => {
                    if state_block.link().is_zero() {
                        false // change block
                    } else {
                        // any block with a link and a balance that hasn't decreased is considered
                        // to be a receive block unless the link points to an epoch account
                        !epochs.is_epoch_link(&state_block.link())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_constants::LEDGER_CONSTANTS_STUB;
    use rsnano_types::{
        ChangeBlock, Link, OpenBlock, PrivateKey, ReceiveBlock, SendBlock, StateBlockArgs,
    };

    #[test]
    fn is_receive_returns_true_for_regular_legacy_receive_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let block: Block = Block::LegacyReceive(ReceiveBlock::new_test_instance());
        let old_balance = Amount::ZERO;
        assert!(is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_true_for_state_receive_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let old_balance = Amount::raw(100);
        let block: Block = StateBlockArgs {
            key: &PrivateKey::from(1),
            previous: 2.into(),
            representative: 3.into(),
            balance: old_balance + Amount::raw(1),
            link: 4.into(),
            work: 0.into(),
        }
        .into();
        assert!(is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_true_for_regular_legacy_open_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let block: Block = Block::LegacyOpen(OpenBlock::new_test_instance());
        let old_balance = Amount::ZERO;
        assert!(is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_false_for_legacy_send_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let block: Block = Block::LegacySend(SendBlock::new_test_instance());
        let old_balance = Amount::ZERO;
        assert!(!is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_false_for_legacy_change_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let block: Block = Block::LegacyChange(ChangeBlock::new_test_instance());
        let old_balance = Amount::ZERO;
        assert!(!is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_false_for_state_change_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let old_balance = Amount::raw(1);
        let block: Block = StateBlockArgs {
            key: &PrivateKey::from(1),
            previous: 2.into(),
            representative: 3.into(),
            balance: old_balance,
            link: Link::ZERO,
            work: 0.into(),
        }
        .into();
        assert!(!is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_false_for_state_send_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let old_balance = Amount::raw(100);
        let block: Block = StateBlockArgs {
            key: &PrivateKey::from(1),
            previous: 2.into(),
            representative: 3.into(),
            balance: old_balance - Amount::raw(1),
            link: 4.into(),
            work: 0.into(),
        }
        .into();
        assert!(!is_receive_block(&block, &old_balance, epochs));
    }

    #[test]
    fn is_receive_returns_false_for_epoch_block() {
        let epochs = &LEDGER_CONSTANTS_STUB.epochs;
        let old_balance = Amount::raw(100);
        let block: Block = StateBlockArgs {
            key: &PrivateKey::from(1),
            previous: 2.into(),
            representative: 3.into(),
            balance: old_balance,
            link: *epochs.link(Epoch::Epoch2).unwrap(),
            work: 0.into(),
        }
        .into();
        assert!(!is_receive_block(&block, &old_balance, epochs));
    }
}
