# RsNano experimental gRPC add-on

This repository contains an independent, experimental gRPC add-on for RsNano,
developed by the OpenRai Initiative. It is maintained separately from upstream
RsNano. The components may become suitable for upstream integration, but this
repository does not assume that integration.

The add-on gives Nano node integrators a versioned Protocol Buffer contract in
addition to JSON-RPC and WebSocket. Generated clients can use typed unary and
server-streaming methods over HTTP/2. JSON-RPC and WebSocket remain the
compatibility baseline for existing integrations.

## Architecture

- [`grpc_proto`](grpc_proto/README.md) owns the versioned `nano.v1` schema and
  generated client and server types.
- [`grpc_server`](grpc_server/README.md) implements that schema against the
  in-process RsNano node and runs as a feature-gated daemon task.
- [`grpc_conformance`](grpc_conformance/README.md) compares gRPC with RsNano
  JSON-RPC against one deterministic development-network ledger and writes the
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
