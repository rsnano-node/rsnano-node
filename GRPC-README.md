# RsNano experimental gRPC add-on

This repository contains an independent, experimental gRPC add-on for RsNano,
developed by the OpenRai Initiative. It is maintained separately from upstream
RsNano. The components may become suitable for upstream integration, but this
repository does not assume that integration.

The add-on gives Nano node integrators a versioned Protocol Buffer contract in
addition to Nano RPC and WebSocket. Generated clients can use typed unary and
server-streaming methods over HTTP/2. Nano RPC and WebSocket remain the
compatibility baseline for existing integrations. Nano RPC is JSON-based but
is not JSON-RPC.

## Protocol scope

`nano.v1` is a public, non-custodial node protocol. It is not a wallet
developer protocol and it does not reproduce Nano RPC's on-node custodial
key-store operations, historically grouped as “Wallet RPCs.” The current
contract accepts already signed blocks with work; it does not expose private
keys, seed phrases, signing, wallet lifecycle, or work generation.

If this project later exposes node-hosted custody, it must use the separately
versioned and independently enabled `nano.custodial_wallet.v1` protocol. The
name means that the server stores or controls wallet secrets; it is not a
general protocol for wallet applications. A remote work provider has a
different authority and cost model, so it belongs in the separate optional
`nano.proof_of_work.v1` protocol, rather than in `nano.v1` or
`nano.custodial_wallet.v1`. Neither protocol exists in this checkout.

## Architecture

- [`grpc_proto`](grpc_proto/README.md) owns the versioned `nano.v1` schema and
  generated client and server types.
- [`grpc_server`](grpc_server/README.md) implements that schema against the
  in-process RsNano node and runs as a feature-gated daemon task.
- [`grpc_conformance`](grpc_conformance/README.md) compares gRPC with RsNano's
  Nano RPC implementation against one deterministic development-network ledger and writes the
  completion report.

The server is in-process: handlers use the same node instance as the existing
servers. This avoids a sidecar process, an IPC hop, and a Protobuf-to-JSON
translation round trip. The schema gives integrators a reviewable contract and
generated types across supported gRPC client ecosystems.

## Required verification

After changing the schema, server, daemon integration, or conformance suite:

```bash
cargo fmt --all
cargo test -p rsnano_grpc_server --lib
cargo test -p rsnano_grpc_conformance --lib
cargo check -p rsnano_cli --bin rsnano --features grpc
./grpc_conformance/run.sh
```

The final command builds the current checkout, runs the suite against an
isolated development node, and rewrites
[`grpc_conformance/reports/current.md`](grpc_conformance/reports/current.md).
It exits non-zero when setup or a scenario fails; always inspect the report.
The conformance guide contains the manual two-terminal workflow and scoring
rules.

## Project documents

- [`GRPC-ROADMAP.md`](GRPC-ROADMAP.md) — milestones, scope boundaries, and
  merge-safe documentation rules.
- [`docs/grpc/new-developer-guide.md`](docs/grpc/new-developer-guide.md) —
  newcomer-oriented explanation of Nano RPC, `nano.v1`, and the optional future
  authority protocols.
- [`docs/grpc/new-developer-guide.html`](docs/grpc/new-developer-guide.html) —
  standalone rendered version of the newcomer guide.
- [`grpc_proto/README.md`](grpc_proto/README.md) — schema and generated types.
- [`grpc_server/README.md`](grpc_server/README.md) — server configuration and
  implementation architecture.
- [`grpc_conformance/README.md`](grpc_conformance/README.md) — deterministic
  verification strategy and coverage procedure.
- [`grpc_conformance/reports/current.md`](grpc_conformance/reports/current.md)
  — current measured parity and coverage state.

## Documentation boundary

Keep add-on documentation in this `GRPC-*` namespace or in its owning gRPC
crate. Upstream-owned root documentation keeps only a short pointer here. This
keeps routine RsNano upstream merges focused on code and avoids recurring
documentation conflicts.
