# RsNano V3.1 Release Notes

This is a small maintenance release containing a ledger correctness fix and an
RPC security fix. Below are the changes that are relevant from a node-operator /
user perspective.

## Ledger fixes

### Correct epoch block validation
Epoch block validation no longer relies on a generic balance-change check.
Instead, receives from the epoch account are now explicitly rejected
(`Unreceivable`). Receive detection is determined by comparing the new balance
against the previous balance, which makes block classification more robust.
Additional unit tests were added covering receive detection and the rejection of
receives from the epoch account.

Thanks to GitHub user **dhyabi2** for reporting this bug.

## RPC security

### `sign` and `wallet_export` now require `enable_control`
The `sign` and `wallet_export` RPC commands now require RPC control to be
enabled, matching the protection already applied to other sensitive commands.
Operators who use these commands must run the RPC server with `enable_control`
turned on.

Thanks to GitHub user **RickiNano** for reporting the missing check.
