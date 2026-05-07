# DEC-017 QUIC Runtime Dependency

## Status

Accepted.

## Context

Andromeda V0 already models the QUIC transport contract in `andromeda-quic`: frame
encoding, stream roles, result sequencing, backpressure, cancellation, connection
lifecycle, surface-plane gating, and RPC dispatch boundaries. These contracts are
intentionally synchronous so protocol/domain tests do not require an async network
runtime.

The next transport steps need a concrete QUIC runtime choice without implementing the
listener, mTLS extraction, executor bridge, or stream manager in this decision. The
choice must preserve existing doctrine:

- QUIC is the transport.
- Protobuf is the boundary serialization format.
- gRPC is forbidden.
- Runtime JSON defaults are forbidden.
- Application, Administration, HA/DR, and Monitoring planes remain separate.
- Catalog, storage, transaction, execution, SRPL, and core crates must not depend on
  network runtime crates.

## Decision

Select `quinn` with `rustls` as the intended QUIC/TLS runtime stack for
Andromeda network I/O, but defer adding the runtime dependencies until the listener
or mTLS work item requires executable wiring.

When the dependencies are introduced, they must be owned by `andromeda-quic` and
guarded behind a non-default Cargo feature. The default `andromeda-quic` build remains
the synchronous model/codec/connection contract surface and must continue to run
without an async QUIC runtime.

The feature name should identify the concrete runtime, for example
`runtime-quinn`. It may enable optional `quinn`, `rustls`, and runtime support
dependencies only inside `andromeda-quic`. Other crates may depend on the typed
transport contracts exported by `andromeda-quic`; they must not depend directly on
`quinn`, `rustls`, or async socket runtime crates as a side effect of this decision.

## Compatibility Constraints

Runtime wiring that follows this decision must satisfy these constraints:

1. **No gRPC surface.** Do not add `tonic`, gRPC code generation, HTTP/2 RPC
   mappings, or gRPC compatibility modes. Andromeda RPC remains its typed frame
   protocol over QUIC streams.
2. **No runtime JSON default.** Protobuf remains the boundary serialization format for
   transport payloads. JSON may not become a runtime transport default.
3. **No generic command or SQL surface.** QUIC carries typed Andromeda frames and
   Protobuf payloads only. It must not expose ad hoc SQL, generic command text, or
   unbounded SRPL execution.
4. **0-RTT disabled for mutating traffic.** The QUIC/TLS configuration must not accept
   early data for mutating Application, Administration, or HA/DR traffic. Until a
   later decision explicitly scopes replay-safe behavior, Andromeda runtime wiring
   should disable early data for all planes.
5. **Plane separation.** Runtime listeners, certificates, routing tags, and dispatch
   must preserve the existing surface planes: Application, Administration, HA/DR, and
   Monitoring. A session is bound to one plane and cross-plane dispatch remains a
   protocol error.
6. **Synchronous contract tests remain runtime-free.** The existing `andromeda-quic`
   model, codec, lifecycle, and backpressure tests must continue to validate without
   enabling the runtime feature.
7. **Dependency ownership remains network-local.** Catalog, storage, transaction,
   execution, SRPL, proto, observe, and core crates must not gain QUIC runtime
   dependencies from this decision.

## Rationale

`quinn` is selected because it is a Rust-native QUIC implementation that integrates
with Rust async runtimes and uses `rustls` for TLS 1.3 configuration. This fits the
Andromeda transport boundary without importing gRPC, HTTP/2 RPC semantics, or JSON
transport conventions.

Deferring dependency introduction preserves the current recoverable vertical slice:
the crate continues to prove frame and session doctrine without coupling protocol
tests to socket scheduling, async executors, certificate loading, or mTLS identity
extraction. The runtime feature gives D2-D5 a clear integration point while keeping
default builds small and deterministic.

Alternatives are rejected for D1:

- Adding `quinn` and `rustls` immediately is premature because D1 does not implement
  listener startup, certificate policy, executor bridging, or stream management.
- Selecting gRPC/`tonic` is forbidden by Andromeda doctrine.
- Selecting HTTP/JSON or JSON-first transport defaults conflicts with the Protobuf
  boundary contract.
- Building a raw UDP protocol instead of QUIC conflicts with the transport doctrine.
- Selecting a non-Rust or FFI-first QUIC stack is unnecessary for the V0 Rust
  workspace and would increase unsafe/runtime integration risk.

## Invariants Preserved

- QUIC remains the network transport for Andromeda.
- Protobuf remains the transport boundary serialization format.
- No gRPC, `tonic`, runtime JSON default, ad hoc SQL, or generic command surface is
  introduced.
- Mutating traffic cannot rely on QUIC 0-RTT.
- Surface planes remain separated at the transport/session boundary.
- `andromeda-quic` remains the only crate that may own concrete QUIC runtime
  dependencies.
- Existing protocol/domain tests remain independent from async runtime wiring.

## Validation

For this decision-only change:

- `cargo test -p andromeda-quic --quiet`

When the `runtime-quinn` feature is introduced in a later work item, add validation
that proves:

- The default `andromeda-quic` build has no `quinn`, `rustls`, `tonic`, gRPC, or
  runtime JSON dependency drift.
- The runtime feature is opt-in and scoped to `andromeda-quic`.
- 0-RTT/early data is disabled for mutating traffic.
- Plane-specific listener routing cannot dispatch frames across plane boundaries.

## Follow-up

- D2: Add a listener-per-plane contract/scaffold without changing the
  synchronous transport contracts; executable listener wiring remains under the
  later non-default runtime feature work.
- D3: Add mTLS certificate extraction and identity binding using the selected
  `rustls` path.
- D4: Add executor bridge boundaries without making execution/catalog/storage/tx
  crates depend on network runtime crates.
- D5: Add stream manager runtime behavior while preserving existing stream role,
  backpressure, cancellation, and result sequence contracts.

## D2 Listener-Per-Plane Contract

D2 adds a runtime-free listener/session configuration scaffold to
`andromeda-quic` without introducing `quinn`, `rustls`, async executor, socket,
or certificate dependencies. The executable runtime remains deferred to later
work under a non-default runtime feature such as `runtime-quinn`.

The listener model is:

- one Application listener configuration;
- one Administration listener configuration;
- one HA/DR listener configuration;
- one Monitoring listener configuration.

Each listener configuration owns exactly one `SurfacePlane` and constructs
sessions already bound to that plane. A connection/session cannot change planes
after construction, and dispatch requests tagged for a different plane remain
protocol errors. Runtime listener wiring must therefore route accepted QUIC
connections through the specific listener configuration that accepted them
rather than through a generic command or SQL surface.

D2 also fixes listener policy defaults for future runtime wiring:

- early data / QUIC 0-RTT is disabled for every plane;
- QUIC datagrams are disabled for every plane in V0 and must not be used as WAL
  shipping, HA/DR transfer, or telemetry shortcut paths;
- protobuf-framed Andromeda transport frames remain the boundary payload;
- no gRPC, HTTP/2 RPC compatibility mode, runtime JSON default, or ad hoc SQL
  surface is introduced.

Still deferred after D2:

- D3 owns certificate loading, mTLS identity extraction, and identity-to-session
  binding;
- D4 owns executor/runtime bridging;
- D5 owns executable stream management and backpressure integration with the
  selected runtime.
