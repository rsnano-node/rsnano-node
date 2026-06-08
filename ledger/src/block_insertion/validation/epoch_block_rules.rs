use super::BlockValidator;
use crate::BlockError;
use rsnano_types::{Block, Epoch, Epochs};

impl<'a> BlockValidator<'a> {
    pub(crate) fn ensure_valid_epoch_block(&self) -> Result<(), BlockError> {
        self.ensure_epoch_block_does_not_change_representative()?;
        self.ensure_epoch_open_has_burn_account_as_rep()?;
        self.ensure_epoch_open_has_pending_entry()?;
        self.ensure_valid_epoch_for_unopened_account()?;
        self.ensure_epoch_upgrade_is_sequential_for_existing_account()?;
        self.ensure_no_receive_from_epoch_account()
    }

    fn ensure_epoch_block_does_not_change_representative(&self) -> Result<(), BlockError> {
        if let Block::State(state) = self.block
            && self.is_epoch_block()
            && let Some(info) = &self.old_account_info
            && state.representative() != info.representative
        {
            return Err(BlockError::RepresentativeMismatch);
        }

        Ok(())
    }

    fn ensure_epoch_open_has_burn_account_as_rep(&self) -> Result<(), BlockError> {
        if let Block::State(state) = self.block
            && self.is_epoch_block()
            && self.block.is_open()
            && !state.representative().is_zero()
        {
            return Err(BlockError::RepresentativeMismatch);
        }
        Ok(())
    }

    fn ensure_epoch_open_has_pending_entry(&self) -> Result<(), BlockError> {
        if self.block.is_open() && self.is_epoch_block() && !self.any_pending_exists {
            return Err(BlockError::GapEpochOpenPending);
        }
        Ok(())
    }

    fn ensure_valid_epoch_for_unopened_account(&self) -> Result<(), BlockError> {
        if self.block.is_open()
            && self.is_epoch_block()
            && self.block_epoch_version() == Epoch::Invalid
        {
            Err(BlockError::BlockPosition)
        } else {
            Ok(())
        }
    }

    fn ensure_epoch_upgrade_is_sequential_for_existing_account(&self) -> Result<(), BlockError> {
        if self.is_epoch_block()
            && let Some(info) = &self.old_account_info
            && !Epochs::is_sequential(info.epoch, self.block_epoch_version())
        {
            return Err(BlockError::BlockPosition);
        }
        Ok(())
    }

    /// If the previous block is missing, we cannot determine whether it is an epoch block
    /// or not. That means we don't know how to check the signature. This precheck just checks
    /// the signature with both the account owner and the epoch signer, because one of them
    /// must be correct.
    /// It's important to abort early with BadSignature, so that the block does not get added
    /// to the unchecked map!
    pub(crate) fn epoch_block_pre_checks(&self) -> Result<(), BlockError> {
        self.ensure_epoch_block_candidate_is_signed_by_owner_or_epoch_account()?;
        self.ensure_previous_block_exists_for_epoch_block_candidate()
    }

    /// This is a precheck that allows for an early return if a block with an epoch link
    /// is not signed by the account owner or the epoch signer.
    /// It is not sure yet, if the block is an epoch block, because it could just be
    /// a normal send to the epoch account.
    pub fn ensure_epoch_block_candidate_is_signed_by_owner_or_epoch_account(
        &self,
    ) -> Result<(), BlockError> {
        if let Block::State(state_block) = self.block {
            // Check for possible regular state blocks with epoch link (send subtype)
            if self.has_epoch_link(state_block)
                && (state_block.verify_signature().is_err()
                    && self.epochs.validate_epoch_signature(self.block).is_err())
            {
                return Err(BlockError::BadSignature);
            }
        }
        Ok(())
    }

    pub fn ensure_previous_block_exists_for_epoch_block_candidate(&self) -> Result<(), BlockError> {
        if let Block::State(state_block) = self.block
            && self.has_epoch_link(state_block)
            && !self.block.previous().is_zero()
            && self.previous_block.is_none()
        {
            return Err(BlockError::GapPrevious);
        }
        Ok(())
    }

    fn ensure_no_receive_from_epoch_account(&self) -> Result<(), BlockError> {
        if self.is_receive()
            && let Block::State(state_block) = self.block
        {
            if self.has_epoch_link(state_block) {
                return Err(BlockError::Unreceivable);
            }
        }
        Ok(())
    }
}
