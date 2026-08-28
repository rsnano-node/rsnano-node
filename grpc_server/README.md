# rsnano_grpc_server

gRPC interface for RsNano, providing a typed, streaming, and language-agnostic API to the Nano node.

## Why gRPC?

RsNano already exposes a Nano RPC server and a WebSocket server. Nano RPC uses
JSON request and response bodies but is not JSON-RPC. A gRPC interface
complements both and addresses a different set of trade-offs that matter when
building application-layer services on top of a node.

### Strongly-typed contracts

Nano RPC uses a proprietary JSON envelope — clients and servers agree on field names
and shapes through documentation alone. gRPC uses Protocol Buffers as a formal
contract. The `.proto` files in `grpc_proto/` are the single source of truth:
both the server and any client (Go, Python, TypeScript, Rust, ...) derive their
types from the same schema. Breaking changes surface at compile time rather than
at runtime in production.

### Bidirectional and server-streaming RPCs

The Nano network is event-driven: blocks confirm, elections start and stop,
telemetry updates arrive continuously. WebSockets can push these events, but the
client must manage its own framing, back-pressure, and reconnection logic. gRPC
the server-streaming event RPCs provide the same push semantics with built-in
flow-control (HTTP/2 windows), deadline propagation, and cancellation — all
first-class concepts in every gRPC client library.

### Code generation eliminates boilerplate

`tonic-build` generates both server traits and fully-featured client stubs from
the `.proto` definitions. Adding a new RPC means writing the server
implementation; the client code, serialisation, and transport are generated.
This keeps the surface area of hand-written code small and reviewable.

### Interoperability and ecosystem

gRPC is a lingua franca across languages and platforms. A monitoring dashboard
in Go, a payment gateway in TypeScript, or a high-throughput ingest pipeline in
Rust can all consume the same node interface without bespoke client libraries.
Server reflection (enabled by default in development) allows tools like
`grpcurl` to explore the API at runtime without pre-compiled stubs.

### Complementary to Nano RPC and WebSocket

The three interfaces serve different audiences:

| Interface | Best for |
|-----------|----------|
| Nano RPC | Ad-hoc tooling, scripts, environments where JSON is the norm |
| WebSocket | Browser clients, lightweight pub/sub with minimal setup |
| gRPC | Backend services, typed clients, streaming workloads, multi-language stacks |

All three read from the same `Arc<Node>` — there is no data duplication or
consistency gap between them.

## Building

The gRPC crates are workspace members and compile unconditionally. The daemon
integration is behind a `grpc` feature flag.

```bash
# Build only the proto and server crates
cargo build -p rsnano_grpc_proto -p rsnano_grpc_server

# Build the daemon with gRPC enabled
cargo build -p rsnano_daemon --features grpc
```

**Prerequisite:** `protoc` (the Protocol Buffers compiler) must be on `PATH`.
On macOS: `brew install protobuf`.

## Testing

```bash
# Unit tests for the server crate (interceptor, config)
cargo test -p rsnano_grpc_server --lib

# Run the full workspace test suite
cargo test --lib -q
```

## Configuration

When the `grpc` feature is active, the daemon looks for `config-grpc.toml` in
the node data directory. All fields are optional; defaults are used when absent.

```toml
# config-grpc.toml
address = "::1"
port = 7078
enable_tls = false
tls_cert_path = "/path/to/cert.pem"
tls_key_path = "/path/to/key.pem"
enable_reflection = true
stream_max_lag = 1024
stream_send_timeout_ms = 500
shutdown_drain_ms = 5000
api_keys = []            # empty = no authentication
```

When `enable_tls` is true, both TLS paths are required and must point to a
PEM-encoded certificate chain and private key. The daemon fails startup if the
files cannot be read, the identity is invalid, or the gRPC address cannot be
bound; it does not silently fall back to plaintext or continue without gRPC.

Default ports by network:

| Network | Port |
|---------|------|
| Live    | 7078 |
| Beta    | 57078 |
| Test    | 17078 |
| Dev     | 47078 |

## Authentication

When `api_keys` contains one or more keys, every gRPC request must include an
`x-api-key` metadata entry matching one of the configured keys. Requests
without a valid key receive `UNAUTHENTICATED`. When the list is empty,
authentication is disabled.

## Architecture

```
daemon
  └─ run_services()
       ├─ run_rpc()              # Nano RPC (existing)
       └─ run_grpc_server()      # gRPC (feature = "grpc")
            ├─ AccountService
            ├─ BlockService
            ├─ LedgerService
            ├─ NetworkService
            ├─ NodeService
            ├─ EventService          (five isolated live event streams)
            └─ tonic-reflection      (dev tooling)
```

Every service handler receives an `Arc<Node>` and calls into the same
infrastructure the Nano RPC and WebSocket servers use. There is no
intermediary layer — the gRPC handlers read directly from the ledger, network,
and telemetry subsystems.

`EventService` exposes confirmation, block-processing, election, vote, and
telemetry observations on independent dispatchers. A slow consumer can exhaust
only its own stream family. Its bounded queue closes with `RESOURCE_EXHAUSTED`;
events are never silently dropped for that subscriber.

These streams are live-only and at-least-once. They have no replay cursor and
make no global or cross-stream ordering promise. `WatchConfirmations` can match
all blocks, exact hashes, or accounts; account matching includes the chain owner
and a ledger-derived linked account for state and historical legacy blocks.
Clients must deduplicate by block hash and reconcile after reconnecting. A
notification is a node observation; only a `CEMENTED` query result says the
block is at or below this node's confirmation height.

## Reliable deposit watcher pattern

A background integration should use the confirmation stream as a wake-up hint,
not as its database. A generated Rust client can watch one account and reconcile
confirmed receivables whenever it connects or receives an event:

```rust,no_run
let filter = ConfirmationFilter {
    mode: Some(confirmation_filter::Mode::Accounts(AccountFilter {
        accounts: vec![deposit_account.clone()],
    })),
};
let mut stream = events.watch_confirmations(WatchConfirmationsRequest {
    filter: Some(filter),
    confirmation_types: vec![], // never filter a correctness-sensitive watcher
    include_election_votes: false,
}).await?.into_inner();

// Run this immediately after every (re)connect too.
reconcile_confirmed_receivables(&mut ledger, &deposit_account).await?;
while let Some(event) = stream.message().await? {
    let Some(block) = event.block else { continue };
    if seen_hashes.insert(block.block_hash) {
        reconcile_confirmed_receivables(&mut ledger, &deposit_account).await?;
    }
}
```

`reconcile_confirmed_receivables` calls `ListReceivables` with
`RECEIVABLE_MODE_CONFIRMED_ONLY` and inserts each result under a unique
`send_block_hash`. There is deliberately no date-ordered transaction feed:
Nano accounts are independent chains in a block lattice.
