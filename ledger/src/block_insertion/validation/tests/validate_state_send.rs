use rsnano_types::{
    AccountInfo, Amount, BlockDetails, BlockSideband, Epoch, PendingInfo, PendingKey,
};

use crate::{BlockError, block_insertion::validation::tests::BlockValidationTest};

#[test]
fn valid_send_block() {
    let test = BlockValidationTest::for_epoch0_account()
        .block_to_validate(|chain| chain.new_send_block().amount_sent(1).build());
    let result = test.assert_is_valid();
    let send_block = &test.block();
    let old_account_info = test.chain.account_info();

    assert_eq!(
        result.account,
        send_block.account_field().unwrap(),
        "account"
    );
    assert_eq!(
        result.set_account_info,
        AccountInfo {
            head: send_block.hash(),
            representative: send_block.representative_field().unwrap(),
            open_block: old_account_info.open_block,
            balance: send_block.balance_field().unwrap(),
            modified: test.now.into(),
            block_count: old_account_info.block_count + 1,
            epoch: old_account_info.epoch,
        }
    );
    assert_eq!(result.delete_pending, None, "delete pending");
    assert_eq!(
        result.insert_pending,
        Some((
            PendingKey::for_send_block(&send_block),
            PendingInfo {
                amount: Amount::raw(1),
                epoch: old_account_info.epoch,
                source: send_block.account_field().unwrap()
            }
        )),
        "insert pending"
    );

    assert_eq!(
        result.set_sideband,
        BlockSideband {
            height: old_account_info.block_count + 1,
            timestamp: test.now,
            account: send_block.account_field().unwrap(),
            balance: send_block.balance_field().unwrap(),
            details: BlockDetails::new(old_account_info.epoch, true, false, false),
            source_epoch: Epoch::Epoch0
        },
        "sideband"
    );
    assert_eq!(result.is_epoch_block, false);
}

#[test]
fn sends_to_burn_account_are_valid() {
    BlockValidationTest::for_epoch0_account()
        .block_to_validate(|chain| chain.new_send_block().link(0).build())
        .assert_is_valid();
}

#[test]
fn send_must_send_at_least_1_raw() {
    BlockValidationTest::for_epoch0_account()
        .block_to_validate(|chain| chain.new_send_block().amount_sent(0).build())
        .assert_validation_fails_with(BlockError::Unreceivable);
}
