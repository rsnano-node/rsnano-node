# rsnano_grpc_proto

Protocol Buffer definitions and generated Rust types for the RsNano gRPC API.

This crate is a **build-time dependency** — it owns the `.proto` schema files and
the `tonic-build` code generation step. Downstream crates (primarily
`rsnano_grpc_server`) depend on the generated server and client traits it
exports.

## Contents

```
proto/
  nano/v1/
    types.proto       # Shared message types (Account, Block, Amount, …)
    services.proto    # Service definitions (AccountService, BlockService, …)
src/
    lib.rs            # Re-exports generated code + FILE_DESCRIPTOR_SET
build.rs              # tonic-build compilation
```

## Regenerating code

The generated Rust code is produced at build time — there is nothing to commit.
To force a regeneration:

```bash
cargo clean -p rsnano_grpc_proto
cargo build -p rsnano_grpc_proto
```

**Prerequisite:** `protoc` must be on `PATH`. On macOS: `brew install protobuf`.

## Design decisions

- **Amounts** are encoded as decimal strings (e.g. `"1000000000000000000000000"`)
  to avoid precision loss across languages that lack native 128-bit integers.
- **Hashes and block IDs** are canonical uppercase hexadecimal strings.
- **Account addresses** use the `nano_` prefix, consistent with the rest of the
  ecosystem.
- **State-block submission** uses typed wire fields. `previous` and `link` are
  canonical uppercase hashes; `balance_raw` is an unsigned decimal raw amount.
  The node derives the multipurpose `link` meaning during normal processing.
- **Block reads are historical-format aware.** `NanoBlock` contains typed state,
  send, receive, open, and change variants. New publication remains state-only.
- **There is no lattice-wide transaction order.** History is explicitly one
  account chain, and local timestamps are diagnostic metadata rather than
  consensus ordering data.
- **Event watches** are live-only, at-least-once local observations. Clients
  deduplicate hashes and reconcile with ledger queries after reconnecting.
  Confirmation-type filters are not for correctness-sensitive accounting.
- **Versioned package path** (`nano.v1`) allows non-breaking evolution of the
  schema alongside the node.

## File descriptor set

The build script also emits a binary file descriptor set, re-exported as
`nano::v1::FILE_DESCRIPTOR_SET`. This is consumed by `tonic-reflection` at
runtime to enable dynamic API exploration via tools like `grpcurl`.
