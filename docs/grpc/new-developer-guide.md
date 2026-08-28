# Nano RPC and the experimental `nano.v1` gRPC interface

This guide helps application developers choose between Nano RPC and RsNano's
experimental gRPC contract.

Nano RPC is the established compatibility baseline. It uses JSON request and
response bodies, but it is a proprietary, non-standard protocol. It is not
JSON-RPC. The `nano.v1` gRPC contract currently covers a selected subset of
public node operations.

## Choose an interface

Use Nano RPC when an application needs its established broad surface, direct
JSON inspection, existing clients, or an operation outside the current gRPC
contract.

Use `nano.v1` when a backend service benefits from generated Protocol Buffer
types, gRPC status codes, and server-streaming methods. Keep Nano RPC available
during adoption. The conformance suite uses RsNano's Nano RPC implementation as
the semantic reference.

Do not treat the current gRPC contract as complete Nano RPC coverage.

## How the protocols reach RsNano

When the daemon enables the `grpc` feature, the gRPC server runs in the RsNano
process. Nano RPC and gRPC handlers read the same node, ledger, network, and
telemetry subsystems. They do not share a wire format or a complete method
inventory.

| Protocol | Request model | Scope |
|---|---|---|
| Nano RPC | An HTTP request contains an `action` and JSON fields. | Established node reads, control, work, on-node custodial key-store operations, diagnostics, and more. |
| `nano.v1` gRPC | Named services accept Protocol Buffer messages. | Selected public account, block, ledger, network, node, and event operations. |

## Current `nano.v1` concept map

The entries below identify the closest Nano RPC concept. They are not promises
of one-to-one request or response shapes.

| gRPC method | Closest Nano RPC concept | Contract distinction |
|---|---|---|
| `GetAccountState` | `account_info` plus receivable and confirmation data | Returns current and confirmed snapshots together. |
| `ListAccountHistory` | `account_history` | Returns typed historical blocks and a continuation hash. |
| `PublishStateBlock` | `process` | Accepts a typed state block. Acceptance is not confirmation. |
| `GetBlock` | `block_info` | Returns typed block data, sideband data, amount, linked account, and local state. |
| `GetBlockStatuses` | Local existence and confirmation queries | Preserves request order and returns `NOT_PRESENT`, `PROCESSED`, or `CEMENTED`. |
| `RequestBlockConfirmation` | `block_confirm` | Reports whether confirmation work was requested. |
| `FrontierCount` | `frontier_count` | Returns a `uint64` count. |
| `ListReceivables` | `receivable` family | Uses an enum for confirmed-only versus all-local selection. |
| `Peers`, `Telemetry` | `peers`, `telemetry` | Grouped under `NetworkService`. |
| `Status`, `Version`, `Keepalive` | Node status, `version`, `keepalive` | Grouped under `NodeService`. |

## Public node access is separate from authority protocols

`nano.v1` is a public, non-custodial node protocol. It is not a protocol for
wallet developers, and it does not reproduce the native Nano RPC operations
historically called “Wallet RPCs.” Those operations act on the node's own key
store. The term “wallet” in that legacy grouping describes custody location,
not an integration category.

`PublishStateBlock` accepts an already signed state block with work. The current
contract does not expose a node-hosted key store, seed phrases, private keys,
signing, custodial-wallet lifecycle, or work generation.

If those authorities are introduced, they are separate protocols:

| Future protocol | Authority | Status |
|---|---|---|
| `nano.proof_of_work.v1` | Remote Proof-of-Work generation and validation. | Not implemented. |
| `nano.custodial_wallet.v1` | A node-hosted key store that controls wallet secrets. It is not a general wallet-application protocol. | Not implemented. |

Each future protocol requires its own listener, enable flag, authorization
model, and conformance record. Neither belongs in the `nano.v1` inventory.

## Event streams require ledger reconciliation

`EventService` provides independent streams for confirmations, block
processing, elections, votes, and telemetry. An event is a live local node
observation. The ledger provides the state an application records.

1. Subscribe to the smallest event family and filter that supports the use case.
2. Treat delivery as live and at least once.
3. Deduplicate events and query the ledger after connection, reconnection, and each relevant event.

Streams have no replay cursor or global ordering. If a subscriber cannot keep
up, its stream closes with `RESOURCE_EXHAUSTED`; the server does not silently
discard events for that subscriber.

## Current verification state

The schema declares 18 methods across six services. Focused Rust validation has
passed. The committed black-box report has not executed any gRPC method because
the Docker build did not reach service startup. It reports `0/18` methods
exercised. The report therefore is not evidence of behavioral parity.

The `nano.v1` roadmap targets included public, non-custodial Nano RPC
operations. It does not target every Nano RPC operation, and its denominator
does not include either future authority protocol.

## Contribute

1. Read [`services.proto`](../../grpc_proto/proto/nano/v1/services.proto) and [`types.proto`](../../grpc_proto/proto/nano/v1/types.proto).
2. Trace the matching handler under [`grpc_server/src/services`](../../grpc_server/src/services/).
3. Compare stable semantics with the existing RsNano Nano RPC handler.
4. Add the method to `ALL_METHODS`; retain `NOT TESTED` until its scenario executes.
5. Run the [required verification loop](../../grpc_conformance/README.md#required-development-loop).

## Sources of truth

- [Add-on overview](../../GRPC-README.md)
- [Roadmap](../../GRPC-ROADMAP.md)
- [Current conformance report](../../grpc_conformance/reports/current.md)
- [Server architecture and configuration](../../grpc_server/README.md)
