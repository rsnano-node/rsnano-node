# RsNano gRPC add-on roadmap

This roadmap covers the independent experimental add-on in this repository. It
does not commit upstream RsNano to integration. The source of measured progress
is [`grpc_conformance/reports/current.md`](grpc_conformance/reports/current.md).

## Target

Provide a typed gRPC interface for relevant, non-deprecated public Nano RPC
operations. For each exposed operation, preserve the Nano RPC behavior that
matters to an integrator while using a typed, versioned gRPC contract. Nano RPC
is JSON-based but not JSON-RPC.

`nano.v1` is deliberately non-custodial. It excludes the native Nano RPC
operations that act on the node's own key store, even though Nano documentation
historically groups them as “Wallet RPCs.” A future node-hosted custody protocol
must be separately versioned and independently enabled as
`nano.custodial_wallet.v1`. The name states that the server controls wallet
secrets; it does not describe a general protocol for wallet applications. A
remote work-provider protocol belongs separately as `nano.proof_of_work.v1`. Neither
future protocol belongs in the `nano.v1` inventory.

The current report scores the 18 methods already in the gRPC contract. It is a
contract-level completion measure, not a percentage of every non-deprecated
Nano RPC operation. Build and maintain the public, non-custodial target
inventory as methods are added; do not reduce the denominator to improve a
score.

## Milestones

1. **Deterministic parity baseline** — maintain the fresh development-network
   fixture, RsNano's Nano RPC implementation as the reference, Markdown report, and everyday 80:20
   scenarios. Keep C++ Nano-node differential testing out of scope.
2. **Everyday unary coverage** — add the most-used non-deprecated account,
   block, ledger, network, and node operations with normal, alternate, and
   common error responses.
3. **Public non-custodial RPC inventory** — extend the contract and
   conformance inventory until every included public Nano RPC operation is
   visible as pass, fail, or not tested.
4. **Complex and streaming behavior** — add deterministic fixtures for the
   operations that need node-hosted custody, peers, elections, confirmations,
   or long-lived streams. Leave each method visibly untested until its premise
   is reproducible.
5. **Optional authority protocols** — decide whether the separate
   `nano.proof_of_work.v1` work-provider and `nano.custodial_wallet.v1` node-hosted
   custody protocols justify their own contracts, listeners, authorization,
   and conformance records. Do not add either surface to `nano.v1` by
   convenience.
6. **Hardening and integration readiness** — improve documentation, operational
   configuration, authentication, and test confidence so individual components
   can be considered for upstream integration on their own merits.

## Contribution rules

For each proposed `nano.v1` operation, confirm that it is non-deprecated,
public, and non-custodial. Then follow the coverage procedure in
[`grpc_conformance/README.md`](grpc_conformance/README.md): update the contract
and implementation, add it to the report inventory, cover everyday success and
error shapes, then commit the refreshed report with the code.

Keep gRPC-specific cross-crate documents in `GRPC-*` files. Keep component
details in `grpc_proto`, `grpc_server`, or `grpc_conformance`. Do not add
substantive gRPC documentation to upstream-owned root files; they retain only a
short pointer to [`GRPC-README.md`](GRPC-README.md).
