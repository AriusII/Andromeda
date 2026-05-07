# DEC-018: mTLS Identity Extraction and Binding

## Status

Approved for Implementation (D3).

## Context

D2 established the listener-per-plane contract and deferred mTLS identity extraction to D3.
The QUIC runtime (quinn + rustls) selected in D1 provides raw peer certificates on accepted
connections. D3 must define:

1. **Certificate extraction** from QUIC/rustls connections: How the raw X.509 certificate
   is obtained from the quinn connection type.
2. **X.509 parsing and fingerprint computation**: Extracting CN/SAN fields and computing
   a stable fingerprint (e.g., SHA256 of DER encoding).
3. **CertificateIdentity binding to session**: Wiring the extracted identity to the
   [`Connection`] state machine so that dispatch authorization can reference it.
4. **Per-plane certificate policy**: Each listener imposes a surface scope requirement
   (e.g., Application listener requires Application-scoped certificates).
5. **Runtime-free contract tests**: Proving that identity extraction is deterministic
   and can be validated without a socket runtime.

## Decision

### 1. Certificate Extraction Model

The certificate extraction logic is owned by `andromeda-quic` and exposed under the
`runtime-quinn` feature. It follows this pipeline:

```
QUIC Connection (quinn::Connection)
  ↓ [extract_peer_cert()]
Raw X.509 bytes (DER)
  ↓ [parse X.509]
Parsed certificate (rustls_pemfile, x509-parser, or manual)
  ↓ [extract identity fields]
(fingerprint: SHA256, subject: CN, surface_scope)
  ↓ [build CertificateIdentity]
CertificateIdentity
  ↓ [bind to Connection.certificate_identity]
Connection with authenticated identity
```

### 2. Identity Extraction Types (andromeda-quic crate)

A new module `andromeda-quic::identity` (runtime-quinn feature-gated) exports:

```rust
/// Raw certificate bytes extracted from a QUIC connection.
pub struct RawCertificate {
    pub der_bytes: Vec<u8>,
}

/// X.509 certificate fields parsed for identity.
#[derive(Clone)]
pub struct ParsedCertificate {
    pub subject_cn: String,
    pub san_dns_names: Vec<String>,
    pub fingerprint_sha256: String,
    pub issuer_cn: Option<String>,
}

impl ParsedCertificate {
    /// Parse from DER-encoded X.509.
    pub fn from_der(der_bytes: &[u8]) -> AndromedaResult<Self> { ... }
    
    /// Extract identity for a surface scope.
    /// Returns Err if certificate subject does not match scope policy.
    pub fn to_certificate_identity(
        self,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<CertificateIdentity> { ... }
}

/// Extract peer certificate from a quinn connection.
pub fn extract_peer_certificate(
    connection: &quinn::Connection,
) -> AndromedaResult<RawCertificate> { ... }
```

### 3. Session-to-Identity Binding

The `Connection` struct (runtime-free) gains an optional certificate field:

```rust
pub struct Connection {
    plane: SurfacePlane,
    state: LifecycleState,
    session_id: Option<SessionId>,
    certificate_identity: Option<CertificateIdentity>,  // NEW
}

impl Connection {
    /// Bind a certificate identity extracted from the QUIC connection.
    /// Callable before or during handshake. Once set, immutable.
    pub fn set_certificate_identity(
        &mut self,
        identity: CertificateIdentity,
    ) -> AndromedaResult<()> {
        if self.certificate_identity.is_some() {
            return Err(protocol_error("certificate identity already bound to this session"));
        }
        // Validate that identity surface matches connection plane.
        if identity.surface as u8 != plane_to_surface_scope(self.plane) as u8 {
            return Err(protocol_error(
                "certificate surface scope does not match connection plane"
            ));
        }
        self.certificate_identity = Some(identity);
        Ok(())
    }

    /// Access the certificate identity bound to this session.
    pub fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        &self.certificate_identity
    }
}
```

### 4. Per-Plane Certificate Policy

Each `SurfaceListenerConfig` encodes a required surface scope:

```rust
impl SurfaceListenerConfig {
    /// The surface scope that certificates on this listener must present.
    pub const fn required_certificate_scope(&self) -> SurfaceScope {
        match self.plane {
            SurfacePlane::Application => SurfaceScope::Application,
            SurfacePlane::Administration => SurfaceScope::Administration,
            SurfacePlane::HighAvailability => SurfaceScope::Cluster,
            SurfacePlane::Monitoring => SurfaceScope::MonitoringAgent,
        }
    }
}
```

When the async listener runtime (D4) accepts a connection:

1. Extract peer certificate from quinn connection.
2. Parse certificate and compute fingerprint.
3. Validate certificate surface scope matches listener's required scope.
4. Build `CertificateIdentity` with fingerprint, subject CN, and scope.
5. Call `connection.set_certificate_identity(identity)` before accepting the `Hello` frame.

### 5. X.509 Parsing Strategy

To minimize dependencies and maintain the `forbid(unsafe_code)` guarantee:

- **Fingerprint computation**: SHA256 of DER-encoded certificate bytes (standard, widely available).
- **Subject CN extraction**: Use `rustls`'s built-in certificate inspection or a lightweight
  parser (e.g., `x509-parser` crate, already available in similar V0 projects).
- **SAN/SPIFFE**: Initially extracted but not required for V0; future admin operations may
  consume SAN for role expansion.

### 6. Dispatch-Time Identity Validation

The dispatch authorization gate (surface_gate.rs) already expects a `presented_fingerprint: &str`.
With D3, the fingerprint now comes from the certificate bound at session construction:

```rust
// In listener runtime (D4):
let fingerprint = connection.certificate_identity()
    .ok_or_else(|| protocol_error("missing certificate identity"))?
    .fingerprint.clone();

// Pass to surface gate:
let outcome = gate.authorize_dispatch(
    trace_id,
    plane,
    &fingerprint,
    action,
)?;
```

Certificate identity is therefore **bound before dispatch**, not looked up at dispatch time.
This satisfies the doctrine: *Surface dispatch authorization must occur before transaction creation.*

## Invariants Preserved

- **mTLS integrity**: Every session is bound to exactly one certificate identity at construction.
- **Cross-plane rejection**: Certificate surface scope mismatch → protocol error before handshake.
- **Fingerprint stability**: SHA256 ensures stable identity across session lifetime.
- **No runtime JSON or ad hoc SQL**: Identity extraction is pure X.509 parsing.
- **Synchronous validation**: Certificate identity validation happens at handshake, not later.
- **Audit traceability**: Fingerprints and subjects appear in all security audit traces.

## Out of Scope (Deferred to Later Decisions)

- **Certificate revocation lists (CRL)**: Placeholder for future security operations.
- **Certificate rotation choreography**: Handled by admin plane operations.
- **SAN role expansion**: Initially parsed but not expanded; future admin decision.
- **Hardware security modules (HSM)**: Deferred; V0 uses rustls + system key material.
- **Mutual TLS for server→server**: V0 focuses on client→server identity; cluster identity
  is deferred to HA/DR decision.

## Validation Criteria

### Contract Tests (Runtime-Free)

1. **Certificate parsing round-trip**: Given a known X.509 certificate in DER format,
   prove that fingerprint, subject, and scope can be extracted deterministically.
2. **Scope mismatch rejection**: Prove that a certificate with surface scope "Administration"
   is rejected on an Application listener.
3. **Session binding immutability**: Prove that once a certificate identity is bound to a
   session, re-binding is rejected.
4. **Dispatch-gate integration**: Prove that `SurfacePlaneAuthorizer` receives the correct
   fingerprint from `Connection.certificate_identity()`.

### Future Runtime Tests (D4, under runtime-quinn feature)

- Prove that quinn peer certificate is extracted and parsed without error.
- Prove that 0-RTT early data is rejected (already enforced by D2 policy).
- Prove that certificate verification failures reject the connection before session creation.

## Implementation Notes

### Crate Dependencies

- `andromeda-quic`: Adds `identity` module (feature-gated).
  - With `runtime-quinn`: depends on `quinn`, `rustls`, and `x509-parser` (or manual parsing).
  - Without `runtime-quinn`: no new dependencies; `Connection.certificate_identity` is `None`.
- `andromeda-observe`: No changes; `CertificateIdentity` already exists.
- `andromeda-exec`: No changes; `SurfaceGate` already accepts fingerprints.

### Testing Strategy

1. **Unit tests in andromeda-quic**: X.509 parsing, fingerprint computation.
2. **Contract tests in andromeda-exec**: `SurfaceGate` contracts that verify identity binding
   before dispatch (via surface_gate_contract.rs).
3. **Integration tests (D4)**: Real quinn+rustls listener wiring.

## Compatibility Notes

- The `Connection` struct gains a new optional field; existing tests that construct
  `Connection` manually must set `certificate_identity = None`.
- D4 listener wiring will require binding identity in the handshake path; tests must
  validate that identity is set before calling `accept_hello()` or before dispatch.

## Follow-up

- **D4 (Executor Bridge)**: Runtime wiring integrates D3 identity extraction into the
  quinn listener loop. Certificate identity becomes mandatory for dispatch (no longer `Option`).
- **D5 (Stream Manager)**: Uses certificate identity from `Connection` to propagate it
  to stream-level audit traces.
- **Future (Cluster)**: HA/DR and server→server mTLS builds on D3 foundation with cluster
  certificate policies.

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| X.509 parsing panic on malformed cert | Parse under try-catch; rustls pre-validates cert before exposing to app. |
| Fingerprint collision (SHA256) | Unlikely in V0; future decision scopes secondary identity (issuer + serial). |
| Scope mismatch not caught until dispatch | D3 validates scope at session construction; D4 enforces before handshake. |
| Certificate identity becomes mutable | Contract tests prove immutability; Rust compiler enforces via `&self` binding. |

## Validation Commands

```bash
# Contract tests (runtime-free):
cargo test -p andromeda-quic --lib identity_parsing --quiet

# Surface gate integration (with identity):
cargo test -p andromeda-exec --test surface_gate_contract --quiet

# Full quic suite (no new dependencies without feature):
cargo test -p andromeda-quic --quiet

# Verify no runtime drift:
cargo test -p andromeda-observe --quiet
```

## Decision Record

This decision establishes the mTLS identity extraction contract for D3. It defers
executable listener wiring (with quinn/rustls socket integration) to D4 but defines
the type and validation surfaces such that D4 can plug in without API changes.
