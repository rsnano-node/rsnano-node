# RsNano gRPC conformance report

This report compares the current gRPC implementation with RsNano JSON-RPC on the same deterministic development-network ledger. Complex wallet, peer-topology, election, and streaming state is intentionally deferred from this first 80:20 suite.

- JSON-RPC endpoint: `http://127.0.0.1:45000`
- gRPC endpoint: `http://127.0.0.1:47078`
- Total gRPC methods: `17`
- Methods exercised: `13`
- Scenarios executed: `21`

## Scoreboard

| Measure | Result | Visualization |
|---|---:|---|
| Method coverage | 13/17 (76%) | `#######--- 76%` |
| Passing methods | 5/17 (29%) | `##-------- 29%` |
| Scenario correctness | 13/21 (61%) | `######---- 61%` |

## Method completion

| Method | Status | Scenarios |
|---|---|---:|
| `AccountService.AccountInfo` | FAIL | 3 |
| `AccountService.AccountBalance` | PASS | 2 |
| `AccountService.AccountHistory` | FAIL | 2 |
| `AccountService.AccountRepresentative` | PASS | 2 |
| `BlockService.Process` | FAIL | 1 |
| `BlockService.BlockInfo` | FAIL | 2 |
| `BlockService.Blocks` | FAIL | 2 |
| `LedgerService.FrontierCount` | PASS | 1 |
| `LedgerService.ReceivableBlocks` | PASS | 2 |
| `NetworkService.Peers` | PASS | 1 |
| `NetworkService.Telemetry` | FAIL | 1 |
| `NodeService.Status` | FAIL | 1 |
| `NodeService.Version` | FAIL | 1 |
| `NodeService.Keepalive` | NOT TESTED | 0 |
| `SubscriptionService.SubscribeConfirmations` | NOT TESTED | 0 |
| `SubscriptionService.SubscribeTelemetry` | NOT TESTED | 0 |
| `SubscriptionService.SubscribeActiveElections` | NOT TESTED | 0 |

## Scenario results

| Method | Scenario | Result | Detail |
|---|---|---|---|
| `AccountService.AccountInfo` | genesis account response | FAIL | representative_block differs: gRPC=``, JSON-RPC=`04270D7F11C4B2B472F2854C5A59F2A7E84226CE9ED799DE75744BD7D85FC9D9` |
| `AccountService.AccountInfo` | invalid account status | PASS | — |
| `AccountService.AccountInfo` | unopened account status | PASS | — |
| `AccountService.AccountBalance` | genesis account balances | PASS | — |
| `AccountService.AccountBalance` | unopened account zero balances | PASS | — |
| `AccountService.AccountHistory` | genesis account history shape | FAIL | type differs: gRPC=`LegacyOpen`, JSON-RPC=`receive` |
| `AccountService.AccountHistory` | invalid account status | PASS | — |
| `AccountService.AccountRepresentative` | genesis representative | PASS | — |
| `AccountService.AccountRepresentative` | unopened account status | PASS | — |
| `BlockService.Process` | invalid block status | FAIL | expected gRPC status InvalidArgument, got Unimplemented: block processing not yet implemented |
| `BlockService.BlockInfo` | genesis block response | FAIL | confirmed differs: gRPC=`false`, JSON-RPC=`true` |
| `BlockService.BlockInfo` | missing block status | PASS | — |
| `BlockService.Blocks` | genesis block response | PASS | — |
| `BlockService.Blocks` | missing block status | FAIL | expected gRPC status NotFound, request succeeded |
| `LedgerService.FrontierCount` | development ledger count | PASS | — |
| `LedgerService.ReceivableBlocks` | empty receivable response | PASS | — |
| `LedgerService.ReceivableBlocks` | invalid account status | PASS | — |
| `NetworkService.Peers` | empty development peer set | PASS | — |
| `NetworkService.Telemetry` | local telemetry response | FAIL | block_count differs: gRPC=`0`, JSON-RPC=`1` |
| `NodeService.Status` | development node status | FAIL | block_count differs: gRPC=`0`, JSON-RPC=`1` |
| `NodeService.Version` | node version response | FAIL | store_version differs: gRPC=`0`, JSON-RPC=`10001` |

## Interpretation

A method passes only when every executed scenario for that method passes. Untested methods remain in the denominator so the completion percentage cannot hide missing behavior. Scenario failures identify semantic differences or incorrect gRPC status mappings; they do not stop report generation.
