use super::BlockValidator;
use crate::BlockError;
use rsnano_types::Block;

impl<'a> BlockValidator<'a> {
    /// If there's no link, the balance must remain the same, only the representative can change
    pub(crate) fn ensure_no_receive_balance_change_without_link(&self) -> Result<(), BlockError> {
        if let Block::State(state) = self.block
            && state.link().is_zero()
            && !self.amount_received().is_zero()
        {
            return Err(BlockError::BalanceMismatch);
        }

        Ok(())
    }

    pub(crate) fn ensure_no_negative_amount_sent(&self) -> Result<(), BlockError> {
        // Is this trying to spend a negative amount (Malicious)
        if let Block::LegacySend(send) = self.block
            && self.previous_balance() < send.balance()
        {
            return Err(BlockError::NegativeSpend);
        }

        Ok(())
    }
}
