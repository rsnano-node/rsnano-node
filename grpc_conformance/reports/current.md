# RsNano gRPC conformance report

This contract-level report compares the current gRPC implementation with RsNano JSON-RPC on the same deterministic development-network ledger. It is not a claim of full Node API coverage; publication and live event scenarios require a running development node.

- JSON-RPC endpoint: `http://127.0.0.1:45000`
- gRPC endpoint: `http://127.0.0.1:47078`
- Total gRPC methods: `18`
- Methods exercised: `0`
- Scenarios executed: `0`

> Suite setup was not completed: the one-shot Docker image build did not reach service startup. Focused Rust tests and the feature-enabled daemon build passed, but black-box results remain unverified.

## Scoreboard

| Measure | Result | Visualization |
|---|---:|---|
| Method coverage | 0/18 (0%) | `---------- 0%` |
| Passing methods | 0/18 (0%) | `---------- 0%` |
| Scenario correctness | 0/0 (0%) | `---------- 0%` |

## Method completion

| Method | Status | Scenarios |
|---|---|---:|
| `AccountService.GetAccountState` | NOT TESTED | 0 |
| `AccountService.ListAccountHistory` | NOT TESTED | 0 |
| `BlockService.PublishStateBlock` | NOT TESTED | 0 |
| `BlockService.GetBlock` | NOT TESTED | 0 |
| `BlockService.GetBlockStatuses` | NOT TESTED | 0 |
| `BlockService.RequestBlockConfirmation` | NOT TESTED | 0 |
| `LedgerService.FrontierCount` | NOT TESTED | 0 |
| `LedgerService.ListReceivables` | NOT TESTED | 0 |
| `NetworkService.Peers` | NOT TESTED | 0 |
| `NetworkService.Telemetry` | NOT TESTED | 0 |
| `NodeService.Status` | NOT TESTED | 0 |
| `NodeService.Version` | NOT TESTED | 0 |
| `NodeService.Keepalive` | NOT TESTED | 0 |
| `EventService.WatchConfirmations` | NOT TESTED | 0 |
| `EventService.WatchBlockProcessing` | NOT TESTED | 0 |
| `EventService.WatchElections` | NOT TESTED | 0 |
| `EventService.WatchVotes` | NOT TESTED | 0 |
| `EventService.WatchTelemetry` | NOT TESTED | 0 |

## Scenario results

| Method | Scenario | Result | Detail |
|---|---|---|---|

## Interpretation

Untested methods remain in the denominator so the report cannot hide missing behavior. This report is a contract inventory and current verification record, not full Nano Node API coverage.
