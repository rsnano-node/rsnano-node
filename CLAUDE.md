# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is RsNano?

RsNano is a full Nano/Banano node written in Rust. It implements the Nano payment protocol with ultrafast transactions and zero fees.

## Commands


### Quickly check for compile errors
```bash
cargo check --tests
```

### Build
```bash
cargo build --all-targets          # build everything including tests
cargo build --bin rsnano_node  # build the node binary
```

### Test
```bash
cargo test --lib -q                # Run only the unit tests. This is the preferred command during development 
cargo test -q                      # run unit tests and integation tests
cargo test -q -p rsnano_node       # run tests for a specific crate
cargo test -q some_test_name       # run a specific test by name
```

### Lint / Format
```bash
cargo fmt --all                    # format all code
cargo fmt --all --check            # check formatting (used in CI)
```

### Run the node
```bash
cargo run --bin rsnano_node -- --network=live node run
```

## Workflow

After finishing editing source files:
1. Run `cargo fmt --all` to format the code.
2. Run `cargo test --lib -q` to verify all unit tests pass.

For gRPC-specific work, start with [GRPC-README.md](GRPC-README.md).

## Code Style

- Prefer `use` statements at the top of a file over inline fully-qualified paths (e.g. `use std::time::Duration` rather than `std::time::Duration` inline).
- Order `mod`/`use` statements at the top of a file in these groups, in this order, with a blank line between groups:
  1. `mod` statements
  2. `pub use` and `pub(crate) use` statements
  3. `use std::` statements
  4. Third-party crate uses (e.g. `use tracing::`)
  5. `rsnano_*` crate uses (e.g. `use rsnano_ledger::`)
  6. Uses for the current crate (e.g. `use crate::consensus::` or `use super::*`)
- In unit tests, use deterministic keys (e.g. `PrivateKey::from(1)`, `PrivateKey::from(2)`) instead of randomly generated ones (e.g. `PrivateKey::new()`), so tests are reproducible and failures are easy to debug.
- Do not duplicate code. When the same logic appears in more than one place, extract it into a shared helper. When you notice duplicated code — whether introduced by an edit or already present — propose a deduplication refactoring before moving on.

## Architecture

### Design Philosophy

The codebase follows **A-frame architecture** and **nullable infrastructure** (testing without mocks) as described by [James Shore](http://www.jamesshore.com/v2/projects/nullables/testing-without-mocks). Understanding these patterns is essential when adding or modifying code.

#### A-Frame Architecture

Logic and Infrastructure are **peers** — neither depends on the other. An Application layer (the "top of the A") coordinates between them. This means:

- **Logic** (pure computation, decisions, state machines) never imports infrastructure types.
- **Infrastructure** (network, disk, clock, database) never contains business logic.
- **Application** code calls infrastructure to read data, passes it to logic, then calls infrastructure to write results (the "Logic Sandwich" pattern).

Example: `BoundedBacklogLogic` contains all rollback decisions. `RollbackLoop` is the application layer that reads from the ledger (infrastructure), drives `BoundedBacklogLogic`, and writes back via the ledger. `Ledger` is pure infrastructure.

#### Infrastructure vs Logic separation rules

- If a struct reads from the network, database, clock, filesystem, or spawns threads → it is **infrastructure**.
- If a struct only takes inputs, computes outputs, and holds state → it is **logic**.
- Logic structs must be unit-testable with no nullable/mock setup.
- Never mix logic and infrastructure in the same struct.

#### Nullable Infrastructure

Infrastructure classes provide a `new_null()` constructor (or `new_null_with_*` variants) that disables all real I/O while keeping the same interface. Tests use nullables instead of mocks — no mocking framework is needed.

Rules:
- Every infrastructure wrapper must have a `new_null()` constructor.
- `new_null()` must have zero side effects (no threads, no file handles, no network connections).
- Nullables behave correctly and run the same logic paths as real implementations.
- Use `OutputTracker` / `OutputTrackerMt` (from `rsnano_output_tracker`) to observe side effects in tests (e.g., what was written, what events fired).

#### Parameterless / Zero-Impact Instantiation

Constructors must not start threads, open connections, or perform I/O. Those belong in a separate `start()` method. This keeps test setup instant and free of side effects.

#### State-Based Tests

Tests assert on **outputs and observable state**, not on which methods were called. Avoid interaction-based assertions. Tests should survive refactoring of internal call sequences.

#### Test Helper Functions

Place test helper functions at the **bottom** of the `mod tests` block, under a `/* Test helpers */` comment. Tests themselves come first, helpers last.

#### Embedded Stubs

Stub implementations live in the **same file** as the production infrastructure code they stand in for, not in separate test files. This keeps the stub and the real implementation in sync.

### Crate Structure

The workspace is split into focused crates:

| Crate | Purpose |
|-------|---------|
| `main` | Node executable entry point |
| `daemon` | Starts node and optional RPC server |
| `node` | Core node implementation (consensus, block processing, bootstrap, transport) |
| `ledger` | Ledger consistency and block validation/insertion/rollback |
| `store_lmdb` | LMDB persistence layer |
| `network` | Manages outbound/inbound TCP channels to other peers |
| `network_protocol` | Handshake, message framing, inbound queue |
| `messages` | Network message types |
| `rpc/server` | RPC server implementation |
| `websocket/server` | WebSocket server implementation |
| `wallet` | Multi-wallet, multi-account management |
| `work` | Proof-of-work generation (CPU/GPU) |
| `types` | Core types: `BlockHash`, `Account`, `KeyPair`, `Vote`, etc. |
| `utils` | Stats, thread pool, ticker, fair queue, cancellation tokens |
| `nullables/*` | Nullable wrappers: `clock`, `condvar`, `fs`, `lmdb`, `tcp`, `random`, `output_tracker`, etc. |
| `tools/test_helpers` | Shared test utilities (e.g., `UnsavedBlockLatticeBuilder`) |

### Node internals (`node` crate)

Major subsystems inside `node/src/`:

- **`block_processing/`** — `BlockProcessor`, `BlockProcessorQueue`, `BoundedBacklog`, `BacklogIndex`, `BacklogScan`, `UncheckedMap`, `LocalBlockBroadcaster`
- **`consensus/`** — Active Elections Container (AEC), election schedulers, vote processing pipeline (`VoteProcessor` → `VoteApplier` → `VoteBroadcaster`), fork cache, vote cache, confirmation solicitor, request aggregator, vote rebroadcast
- **`bootstrap/`** — `Bootstrapper`, `BootstrapServer`, bootstrap election activator
- **`cementation/`** — `ConfirmingSet`, confirmation time tracking
- **`transport/`** — Peer connectors, channel management
- **`telemetry/`** — Node telemetry collection

### Ledger internals (`ledger` crate)

- **`block_insertion/`** — `BlockValidator` checks rules → `BlockInserter` follows `BlockInsertInstructions`
- **`block_rollback/`** — `RollbackPlanner` plans rollback → `RollbackInstructionsExecutor` executes it
- **`ledger_sets.rs`** — `LedgerSet` views: confirmed, unconfirmed, any

### Nullable infrastructure pattern

Each infrastructure concern has a nullable wrapper crate under `nullables/`:
- `rsnano_nullable_clock` — `SteadyClock`, `SystemTimeFactory`
- `rsnano_nullable_lmdb` — `LmdbEnvironment`, `LmdbEnvironmentFactory`
- `rsnano_nullable_fs` — `NullableFilesystem`
- `rsnano_nullable_condvar` — `NullableCondvarMutex`
- `rsnano_output_tracker` — `OutputTracker`/`OutputTrackerMt` for recording outputs in tests

In tests, use `Ledger::new_null()` and `*::new_null()` constructors to get in-memory/stub implementations. Use `UnsavedBlockLatticeBuilder::with_stub_work()` from `tools/test_helpers` to build test block chains.

### Threading model

Long-running work uses `ThreadPool` (from `rsnano_utils::thread_pool`) and `TickerPool`/`TimerThread` for periodic tasks. `CancellationToken` is used for cooperative shutdown. Background threads use `backpressure_channel` for flow control.
