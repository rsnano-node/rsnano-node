# RsNano gRPC add-on roadmap

This roadmap covers the independent experimental add-on in this repository. It
does not commit upstream RsNano to integration. The source of measured progress
is [`grpc_conformance/reports/current.md`](grpc_conformance/reports/current.md).

## Target

Provide a standard gRPC interface for every Nano JSON-RPC method that is not
marked deprecated in the authoritative Nano documentation. For each exposed
operation, preserve the JSON-RPC behavior that matters to an integrator while
using a typed, versioned gRPC contract.

The current report scores the 17 methods already in the gRPC contract. It is a
contract-level completion measure, not yet a percentage of the full
non-deprecated Nano RPC surface. Build and maintain that full target inventory
as methods are added; do not reduce the denominator to improve a score.

## Milestones

1. **Deterministic parity baseline** — maintain the fresh development-network
   fixture, RsNano JSON-RPC reference, Markdown report, and everyday 80:20
   scenarios. Keep C++ Nano-node differential testing out of scope.
2. **Everyday unary coverage** — add the most-used non-deprecated account,
   block, ledger, network, and node operations with normal, alternate, and
   common error responses.
3. **Full non-deprecated RPC inventory** — extend the contract and conformance
   inventory until every supported non-deprecated Nano RPC is visible as pass,
   fail, or not tested.
4. **Complex and streaming behavior** — add deterministic fixtures for the
   operations that need wallet state, peers, elections, confirmations, or
   long-lived streams. Leave each method visibly untested until its premise is
   reproducible.
5. **Hardening and integration readiness** — improve documentation, operational
   configuration, authentication, and test confidence so individual components
   can be considered for upstream integration on their own merits.

## Contribution rules

For each new RPC, follow the coverage procedure in
[`grpc_conformance/README.md`](grpc_conformance/README.md): confirm it is not
deprecated, update the contract and implementation, add it to the report
inventory, cover everyday success and error shapes, then commit the refreshed
report with the code.

Keep gRPC-specific cross-crate documents in `GRPC-*` files. Keep component
details in `grpc_proto`, `grpc_server`, or `grpc_conformance`. Do not add
substantive gRPC documentation to upstream-owned root files; they retain only a
short pointer to [`GRPC-README.md`](GRPC-README.md).
