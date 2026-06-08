use crate::{BlockError, block_insertion::validation::tests::BlockValidationTest};
use rsnano_types::{AccountInfo, BlockDetails, BlockSideband, Epoch};

#[test]
fn valid_change_block() {
    let test = BlockValidationTest::for_epoch0_account()
        .block_to_validate(|chain| chain.new_state_block().representative(12345).build());
    let result = test.assert_is_valid();
    let change_block = test.block();
    let old_account_info = test.chain.account_info();

    assert_eq!(
        result.set_account_info,
        AccountInfo {
            head: change_block.hash(),
            representative: change_block.representative_field().unwrap(),
            open_block: old_account_info.open_block,
            balance: old_account_info.balance,
            modified: test.now.into(),
            block_count: old_account_info.block_count + 1,
            epoch: old_account_info.epoch,
        }
    );
    assert_eq!(result.delete_pending, None, "delete pending");
    assert_eq!(result.insert_pending, None, "insert pending");

    assert_eq!(
        result.set_sideband,
        BlockSideband {
            height: old_account_info.block_count + 1,
            timestamp: test.now,
            account: change_block.account_field().unwrap(),
            balance: old_account_info.balance,
            details: BlockDetails::new(old_account_info.epoch, false, false, false),
            source_epoch: Epoch::Epoch0
        },
        "sideband"
    );
    assert_eq!(result.is_epoch_block, false);
}

#[test]
fn change_block_with_link_is_rejected() {
    BlockValidationTest::for_epoch2_account()
        .block_to_validate(|chain| {
            chain
                .new_state_block()
                .representative(12345)
                .link(456)
                .build()
        })
        .assert_validation_fails_with(BlockError::Unreceivable);
}
