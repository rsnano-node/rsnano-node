# RsNano gRPC conformance suite

For the cross-crate architecture and roadmap, see
[`GRPC-README.md`](../GRPC-README.md) and
[`GRPC-ROADMAP.md`](../GRPC-ROADMAP.md).

This black-box suite compares the experimental gRPC interface with RsNano's
JSON-RPC interface on the same deterministic development-network ledger. It is
the initial completion scoreboard for the gRPC add-on; cross-implementation
comparison with the C++ Nano node is intentionally out of scope.

The suite targets the draft contract's reconciliation surface: atomic account
state, account-chain-local history, typed block lookup, ordered local status
queries, confirmation requests, ledger counts, and receivables. Publication and
live confirmation/election/vote/telemetry behavior remain visible as
`NOT TESTED` rather than disappearing from the completion denominator. It is a
contract-level report, not a claim of full Node API coverage.

## Required development loop

Run this sequence after changing `grpc_proto`, `grpc_server`, the daemon's gRPC
integration, or this suite:

```bash
cargo fmt --all
cargo test -p rsnano_grpc_server --lib
cargo test -p rsnano_grpc_conformance --lib
cargo check -p rsnano_cli --bin rsnano --features grpc
./grpc_conformance/run.sh
```

The first four commands catch formatting, unit, report-model, and feature-gate
failures. The final command verifies observable behavior on a fresh development
ledger and rewrites `reports/current.md`.

The conformance command exits non-zero if setup or any scenario fails. This is
expected while the report contains known parity gaps. A failed exit does not
replace report review: confirm that every new failure is understood and that
the coverage and completion counts changed as intended.

## Container verification

Docker Compose builds the feature-enabled RsNano node and the conformance
runner from the current checkout, starts an isolated development node, runs all
scenarios, and writes the report to `reports/current.md`.

```bash
./grpc_conformance/run.sh
```

The command exits non-zero when any scenario fails, but it still writes the
Markdown report. This makes current gaps visible while preserving a CI-friendly
failure signal.

## Manual two-terminal verification

Use this path to debug the node or runner outside Docker. It uses the same
configuration and deterministic development-network genesis as the container
path. `protoc` must be available on `PATH`.

Build the feature-enabled node and the runner:

```bash
cargo build -p rsnano_cli --bin rsnano --features grpc
cargo build -p rsnano_grpc_conformance --bin grpc-conformance
```

In terminal 1, create a fresh data directory and start RsNano:

```bash
export RSNANO_GRPC_DATA="$(mktemp -d)"
cp grpc_conformance/container/config/*.toml "$RSNANO_GRPC_DATA/"
./target/debug/rsnano \
  --network=dev \
  --data-path="$RSNANO_GRPC_DATA" \
  node run \
  --disable-rep-crawler
```

In terminal 2, run the suite against both interfaces:

```bash
./target/debug/grpc-conformance \
  --rpc-url http://127.0.0.1:45000 \
  --grpc-url http://127.0.0.1:47078 \
  --report grpc_conformance/reports/current.md
```

The runner waits up to 30 seconds for both endpoints. Stop and restart the node
after rebuilding server or daemon code. Rebuild the runner after changing a
scenario or report logic. Always use a fresh data directory for the committed
scoreboard so prior manual activity cannot change the fixture.

## Verification invariants

Keep these rules intact when extending the suite:

1. Use RsNano JSON-RPC as the behavioral reference for this phase.
2. Send JSON-RPC and gRPC requests to the same fresh RsNano process.
3. Use the development network; never sync or depend on the mainnet lattice.
4. Compare stable semantics and explicitly normalize intentional representation
   differences. Do not ignore a field merely to make a scenario pass.
5. Verify common success shapes, alternate shapes, and gRPC status mappings.
6. Write the report even when setup or scenarios fail.
7. Keep untested methods visible. Never count an unexecuted method as passing.
8. Do not add C++ Nano node differential testing to this suite at this stage.

## Scoring

The report publishes three separate measures:

- **Method coverage** is the number of contract methods with at least one
  executed scenario divided by the current method inventory.
- **Passing methods** is the number of methods whose executed scenarios all
  pass divided by the current method inventory.
- **Scenario correctness** is the number of passing scenarios divided by the
  number executed.

`ALL_METHODS` in `src/lib.rs` is the current inventory and report denominator.
It currently contains the 18 methods present in the draft gRPC contract.
Therefore, the passing-method percentage is contract completion rather than
overall coverage of every Nano RPC. Grow the inventory as the contract expands,
and do not remove entries to improve the score.

## Adding coverage

For each incremental RPC addition:

1. Confirm that the Nano RPC is not marked deprecated.
2. Add or update the Protocol Buffer contract and gRPC implementation.
3. Add the method to `ALL_METHODS` in the same change.
4. Add a representative everyday success scenario.
5. Add the most common alternate response or error-status scenario.
6. Compare only stable fields required by the contract.
7. Run the required development loop.
8. Review and commit the updated `reports/current.md` with the code.

Prioritize the everyday 80:20 surface before fixtures that require wallets,
multiple peers, active elections, or long-lived streams. When complex state is
deferred, keep the method as `NOT TESTED` and make the missing premise clear in
the scenario plan or change description.
