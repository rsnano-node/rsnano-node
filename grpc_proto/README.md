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
- **Hashes and block IDs** are hex-encoded strings, matching the convention used
  by the existing JSON-RPC interface.
- **Account addresses** use the `nano_` prefix, consistent with the rest of the
  ecosystem.
- **Versioned package path** (`nano.v1`) allows non-breaking evolution of the
  schema alongside the node.

## File descriptor set

The build script also emits a binary file descriptor set, re-exported as
`nano::v1::FILE_DESCRIPTOR_SET`. This is consumed by `tonic-reflection` at
runtime to enable dynamic API exploration via tools like `grpcurl`.
