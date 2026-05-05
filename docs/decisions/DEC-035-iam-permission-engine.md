# DEC-035: IAM and Permission Engine for V0.5

**Artifact Type:** Architecture Decision Record  
**Wave:** 19 (N6)  
**Status:** Design (Ready for Implementation)  
**Primary Decider:** Security IAM Auditor  
**Last Updated:** [Timestamp]  

---

## Executive Summary

This decision record specifies the complete Identity & Access Management (IAM) pipeline for Andromeda V0.5:

1. **Principal Model** — User and service identities derived from mTLS certificates
2. **Permission Model** — Role-Based Access Control (RBAC) at procedure and administrative operation scope
3. **Authorization Contract** — Certificate → Principal → Permission → Audit decision point
4. **Session Model** — Request-scoped identity binding (no persistent session tokens in V0)
5. **Audit Trail** — Immutable, append-only authorization decision log with forensic traceability
6. **Default Posture** — Default-deny (no permission = no access)

This design integrates with existing infrastructure:
- **DEC-018** (mTLS identity extraction): Certificate fingerprint → CertificateIdentity
- **DEC-027** (Audit taxonomy): SecurityAuditTrace, TraceId, principal binding
- **Principal Binding Module** (andromeda-observe): PrincipalBinding, UserPrincipal, Permission types

**No implementation code in this wave.** All APIs are contract-only (`todo!()` bodies).

---

## Context

### Prior Work

**Wave 16 (DEC-018) — mTLS Identity Extraction:**
- Established certificate extraction from QUIC connections
- Defined CertificateIdentity (fingerprint, subject CN, SAN, issuer)
- Bound identity to Connection.certificate_identity
- Scoped certificates per SurfacePlane (Application, Administration, Cluster)

**Wave 17–18 (Partial):**
- andromeda-observe began principal_binding.rs with PrincipalBinding struct
- Defined SurfaceScope, SurfaceAction, Permission enums
- Outlined authorization outcome and SecurityAuditTrace binding

### Gap Analysis

**Missing:**
1. Formal permission model specification (RBAC vs ABAC decision)
2. Role hierarchy and group semantics
3. Session semantics (lifetime, revocation, re-authentication)
4. Audit permanence guarantees (write-once storage, retention policy)
5. Admin operation permission matrix
6. Permission inheritance rules (procedure vs package vs catalog scope)
7. Default-deny enforcement at dispatch boundary

**Constraints:**
- No persistent session tokens (out of scope for V0)
- No ad hoc SQL surface (no permission for ad hoc queries)
- No gRPC (protocol already locked, mTLS via QUIC only)
- No changes to certificate/mTLS pipeline (DEC-018 locked)

---

## Design Decisions

### 1. RBAC Model (Role-Based Access Control) — Selected over ABAC

**Decision:** Use Role-Based Access Control (RBAC) with flat role set in V0.

**Rationale:**
- **ABAC (Attribute-Based)** allows dynamic attribute predicates (e.g., "if cert_country == 'US' AND timestamp < expiry").
  - Requires runtime evaluation engine.
  - Risk of silent evaluation errors (hidden attributes, race conditions).
  - Out of scope for V0.
  
- **RBAC (Role-Based)** uses fixed role enumeration and lookup.
  - Roles pre-computed and stored in PrincipalBinding.
  - Permission grant is deterministic (lookup success = permit).
  - Audit trail binds role name to permission.
  - Easier to audit and reason about.

**Semantics:**
- A UserPrincipal carries zero or more Roles.
- Each Role grants a fixed set of Permissions.
- Permissions are evaluated via role lookup, not attribute evaluation.
- Role grant is immutable within a session (no runtime role changes).

**Implication for Wave 19:**
- Roles are enumerated at design time (not dynamically created in V0).
- Role hierarchy deferred to Wave 20+ (V0.6).
- Group expansion deferred.

---

### 2. Permission Granularity — Procedure-Level (not Operation-Level)

**Decision:** Grant permissions at *procedure scope*, not individual operation scope.

**Rationale:**
- **Operation-Level:** Separate permission for each operation (Create, Read, Update, Delete, Execute).
  - 5–10× more permission checks in dispatch path.
  - Audit trail verbosity (each operation logged separately).
  - Complexity in role definition (role = set of operations).
  - Out of scope for V0.

- **Procedure-Level:** Single permission grant covers all operations on a procedure.
  - Grant: "ExecuteProcedure on payroll_summary"
  - Deny: "No ExecuteProcedure on admin_shutdown"
  - Simpler role definition (role = set of procedures).
  - Matches contract execution model (procedure is unit of work).
  - Audit trail clarity (one trace per procedure invocation).

**Permission Set (V0):**
```
- ExecuteProcedure(procedure_id: u64)    // Invoke a catalog procedure
- ReadContractMetadata                    // Read published contracts (no data access)
- AdminRoleMgmt                           // Create/revoke role grants
- AdminCatalogPublish                     // Publish/unpublish procedures
- AdminShutdown                           // Stop server
- AdminRecovery                           // Trigger restore/recovery
- AdminAuditRead                          // Query audit log
- AdminCertRotate                         // Replace certificates
```

**Evolution Path for V0.6+:**
- If operation-level granularity needed, add OperationPermission variant.
- Backward-compatible if Permission enum is extended (not changed).

---

### 3. Permission Scope Hierarchy

**Decision:** Permissions organize in a three-level hierarchy (no inheritance in V0).

```
CatalogLevel (global)
├── ProcedureLevel (specific to procedure_id)
│   ├── Execute privilege
│   └── Audit privilege (operator surface)
├── AdminLevel (privileged operations)
│   ├── Cluster management
│   ├── Catalog management
│   ├── Role management
│   └── Backup/recovery
└── ReadLevel (read-only)
    ├── Contract metadata
    └── Audit trails (limited)
```

**Rule: No inheritance.**
- Grant at one level does not cascade to another.
- Role "AdminCatalogPublish" does NOT grant "ExecuteProcedure" on all procedures.
- Explicit permission required at each scope.

**Rationale:**
- Prevents privilege escalation via inheritance chains.
- Audit trail clarity: exactly which permission was checked.
- Simpler policy engine (no resolution algorithm needed).

---

### 4. Session Model — Request-Scoped Identity Binding

**Decision:** Sessions are request-scoped. No persistent session tokens in V0.

**Rationale:**
- **Persistent Tokens** (e.g., JWT, OAuth tokens):
  - Require token storage and revocation tracking.
  - Adds time-based expiry management.
  - Requires re-authentication on token expiry.
  - Out of scope for V0 (defer to WAM/session management wave).

- **Request-Scoped Binding:**
  - Certificate identity extracted once per QUIC connection.
  - Principal binding looked up once per frame arrival.
  - Permissions checked at dispatch boundary.
  - Identity valid for duration of connection.
  - Revocation requires connection close + re-open.

**Semantics:**
```
Connection established
  ↓ (once per connection)
Extract peer certificate → compute fingerprint
  ↓ (once per frame)
Lookup principal binding in PrincipalRegistry
  ↓ (once per frame)
Authorize action via SurfaceAuthorizer
  ↓ (emit SecurityAuditTrace)
Dispatch frame or deny
  ↓ (on connection close)
Session ends; certificate identity cleared
```

**Session Lifetime:**
- Starts: QUIC connection accepted
- Ends: QUIC connection closed
- No explicit session token lifetime (connection-bound)
- Certificate must be valid (no expiry check in V0; assume valid certs only)

**Revocation Model (V0 Limitation):**
- Certificate revocation requires connection close.
- No CRL (Certificate Revocation List) check in V0.
- Deferred to Wave 20+ (add CRL/OCSP support).

---

### 5. Audit Trail — Immutable, Append-Only, Principal-Bound

**Decision:** Authorization and admission decisions are immutable audit events with forensic traceability.

**Audit Requirements:**

#### 5.1 Immutability
- Audit events are write-once (never mutated after emission).
- Stored in an append-only log (e.g., file, WAL, or event sink).
- No retroactive modification or deletion of audit records.

#### 5.2 Append-Only Semantics
- All authorization decisions, allow and deny, are logged.
- Events carry monotonic EventId (no gaps).
- Chronological ordering preserved via TraceId + timestamp.

#### 5.3 Principal Binding
- Every audit event carries:
  - **certificate:** CertificateIdentity (subject CN, fingerprint)
  - **principal:** UserPrincipal ID and roles
  - **surface_scope:** Application, Administration, Cluster, BackupAgent
  - **action:** SurfaceAction (ExecuteProcedure, ReadContract, Admin(op))
  - **permission:** Permission required by the action
  - **outcome:** Allowed or Denied
  - **reason:** Typed denial reason (missing permission, invalid certificate, etc.)

#### 5.4 No Silent Drops
- If authorization check fails, a SecurityAuditTrace is **always** emitted.
- If emission fails (disk full, sink saturated), an error is returned to the caller.
- EventEmitter tracks rejected_count() for monitoring.

#### 5.5 Audit Event Schema
```rust
pub struct SecurityAuditTrace {
    pub trace_id: TraceId,
    pub timestamp: SystemTime,
    pub surface_scope: SurfaceScope,
    pub certificate_identity: CertificateIdentity,
    pub user_principal: UserPrincipal,
    pub action: SurfaceAction,
    pub required_permission: Permission,
    pub outcome: SecurityAuditOutcome,        // Allowed | Denied
    pub reason: String,                        // e.g., "permission granted" or "no such role"
}

pub enum SecurityAuditOutcome {
    Allowed,
    Denied,
}
```

#### 5.6 Permanence Guarantees
- **V0:** Audit events stored in in-memory EventEmitter (not persisted to disk).
  - Acceptable for short-lived test/demo workloads.
  - Not suitable for production deployments.

- **V0.5+:** Audit events must be persisted to durable storage.
  - Written to a dedicated audit log file (separate from WAL).
  - Format: append-only, each event immutable.
  - Retention policy: indefinite (no purge in V0.5).

- **V1.0+:** Add compliance features.
  - Cryptographic signing of audit records.
  - Remote audit log replication.
  - Tamper detection and verification.

---

### 6. Default-Deny Posture

**Decision:** No permission → no access. Default posture is deny.

**Principle:**
- If a principal is not granted a permission, the action is denied.
- There is no "grant all" or wildcard permission in V0.
- Every action requires an explicit permission grant.

**Enforcement:**
```rust
pub fn authorize(&self, principal: &UserPrincipal, permission: &Permission) -> bool {
    // Check if principal's roles include a role that grants this permission.
    principal.roles.iter().any(|role| {
        self.role_permissions[&role].contains(permission)
    })
    // If no role grants it, return false (default-deny).
}
```

**Implication:**
- Deny is safer than allow (assume attacker if in doubt).
- No risk of "accidentally granting too much" via default.
- Audit trail clarity: every grant is explicit.

---

### 7. Role Hierarchy — Flat Set (No Inheritance in V0)

**Decision:** Roles are a flat set. No role hierarchy (parent-child, super-roles) in V0.

**Rationale:**
- Flat roles are simpler to audit (no transitive grants).
- Prevent privilege escalation via inheritance chains.
- Role matrix is explicit (role → set of permissions, visible at a glance).

**Role Examples (V0):**
```
role_application_user
├── ExecuteProcedure(*)     // Any procedure
└── ReadContractMetadata

role_cluster_admin
├── AdminRoleMgmt
├── AdminCertRotate
└── AdminShutdown

role_catalog_publisher
├── AdminCatalogPublish
└── AdminCatalogUnpublish

role_audit_reader
├── AdminAuditRead
└── (no write permissions)

role_system_recovery
├── AdminRecovery
└── AdminShutdown
```

**Flat Lookup:**
```rust
pub fn role_permissions() -> HashMap<Role, Vec<Permission>> {
    // Single-level lookup; no inheritance resolution.
}
```

**Evolution Path (V0.6+):**
- If role hierarchy needed, add parent_roles field to Role struct.
- Recursively expand parent roles during permission lookup.
- Audit trace captures final expanded set.

---

### 8. Permission Binding — Immutable Within Session

**Decision:** Once a principal is bound to a session (via certificate fingerprint), the permission set is fixed for the duration of the connection.

**Rationale:**
- QUIC connections are long-lived (reuse many frames).
- Lookup permission set once per connection, not per frame.
- Avoid race conditions (permission revoked mid-request).
- Simpler to audit (session permission set static).

**Enforcement:**
```rust
pub struct Connection {
    // ...
    certificate_identity: Option<CertificateIdentity>,
    principal_binding: Option<PrincipalBinding>,    // Immutable after set
}

impl Connection {
    pub fn set_principal_binding(&mut self, binding: PrincipalBinding) {
        // Can only be set once; subsequent calls return error.
    }
}
```

**Revocation Semantics:**
- To revoke permissions, close the connection.
- Client reconnects and new principal binding is evaluated.
- If certificate is no longer in PrincipalRegistry, new connection fails.

---

### 9. Certificate and Identity Scope Validation

**Decision:** Enforce SurfaceScope policy per listener (input from DEC-018).

**Scope Rules:**
| Surface | Required Scope | Procedure Access | Admin Access |
|---------|---|---|---|
| Application | application-scoped cert | ExecuteProcedure | None |
| Administration | admin-scoped cert | None | AdminRoleMgmt, AdminCatalogPublish, AdminShutdown |
| Cluster | cluster-scoped cert | None | AdminRecovery, membership ops |
| BackupAgent | backup-scoped cert | None | BackupAgent-only ops |

**Enforcement:**
```rust
pub fn check_surface_scope_policy(
    certificate: &CertificateIdentity,
    surface: SurfaceScope,
) -> AndromedaResult<()> {
    if !certificate.scopes().contains(&surface) {
        return Err(security_error("certificate invalid for surface"));
    }
    Ok(())
}
```

**Implication:**
- A certificate valid for Application scope cannot be used for Administration.
- Listener rejects connection if certificate scope mismatches.
- No cross-scope privilege escalation possible.

---

### 10. Principal Identity and Evidence Validation

**Decision:** A valid UserPrincipal and CertificateIdentity must carry identity evidence (non-empty CN or SAN).

**Validation:**
```rust
pub trait HasIdentityEvidence {
    fn has_identity_evidence(&self) -> bool;
}

impl HasIdentityEvidence for CertificateIdentity {
    fn has_identity_evidence(&self) -> bool {
        !self.subject_cn.is_empty() || !self.san_dns_names.is_empty()
    }
}

impl HasIdentityEvidence for UserPrincipal {
    fn has_identity_evidence(&self) -> bool {
        !self.principal_id.is_empty() && self.principal_id.len() <= 256
    }
}
```

**Rationale:**
- Prevents binding to anonymous or empty identities.
- Audit trail must contain interpretable identity information.
- Fingerprint alone is insufficient (digest collision risk).

---

## Permission Model Definition

### Standard Permission Set

| Permission | Target Scope | Intent | Granter |
|---|---|---|---|
| ExecuteProcedure(proc_id) | Procedure | Invoke a catalog procedure | Catalog admin |
| ReadContractMetadata | Global | Query published contracts | Catalog admin |
| AdminRoleMgmt | Global | Grant/revoke role memberships | System admin |
| AdminCatalogPublish | Global | Publish/unpublish procedures | Catalog admin |
| AdminShutdown | Global | Stop the server | System admin |
| AdminRecovery | Global | Trigger restore/recovery | System admin |
| AdminAuditRead | Global | Query audit log | Compliance officer |
| AdminCertRotate | Global | Install new certificates | Security admin |

### Permission Composition (Examples)

**Application User Role:**
```
Permissions:
  - ExecuteProcedure(payroll_summary)
  - ExecuteProcedure(employee_query)
  - ReadContractMetadata
```

**Catalog Administrator Role:**
```
Permissions:
  - AdminCatalogPublish
  - AdminCatalogUnpublish
  - AdminAuditRead
```

**System Administrator Role:**
```
Permissions:
  - AdminRoleMgmt
  - AdminShutdown
  - AdminRecovery
  - AdminCertRotate
  - AdminAuditRead
```

---

## Authorization Decision Flow

### Decision Path

```
1. Connection accepted
   ├─ Extract peer certificate (DEC-018)
   └─ Bind to Connection.certificate_identity

2. Frame arrives on established connection
   ├─ Decode frame header
   ├─ Extract SurfaceScope from frame
   └─ Proceed to step 3

3. Authorization check
   ├─ Lookup PrincipalBinding by certificate fingerprint
   │  ├─ If not found: emit Denied trace, close connection
   │  └─ If found: proceed to step 4
   └─ [Permission lookup not yet implemented in V0]

4. Permission decision
   ├─ Extract required permission from action
   ├─ Check if any of principal's roles grant the permission
   │  ├─ If yes: emit Allowed trace
   │  └─ If no: emit Denied trace
   └─ (Both paths emit SecurityAuditTrace)

5. Dispatch or reject
   ├─ If Allowed: process frame
   └─ If Denied: return error to client (no frame processed)
```

### Audit Trace Emission

**Allow Path:**
```rust
SecurityAuditTrace {
    trace_id,
    surface_scope,
    certificate_identity,
    user_principal,
    action: SurfaceAction::ExecuteProcedure,
    required_permission: Permission::ExecuteProcedure(proc_id),
    outcome: SecurityAuditOutcome::Allowed,
    reason: "role 'application_user' grants ExecuteProcedure",
}
```

**Deny Path:**
```rust
SecurityAuditTrace {
    trace_id,
    surface_scope,
    certificate_identity,
    user_principal,
    action: SurfaceAction::Admin(AdminShutdown),
    required_permission: Permission::AdminShutdown,
    outcome: SecurityAuditOutcome::Denied,
    reason: "no role grants AdminShutdown",
}
```

---

## Session Token and Lifetime Semantics

### V0 — Request-Scoped (No Persistent Tokens)

**Session Definition:**
- Begins: QUIC connection accepted + certificate bound
- Ends: QUIC connection closed
- Token: None (identity is tied to connection)

**Implications:**
- Client reconnects for every logical session.
- No "login" phase (authenticate via certificate on each connection).
- No token expiry (session lifetime = connection lifetime).
- Simple and auditable.

### Future (V0.6+ — Not in Scope)

**Persistent Session Tokens (Deferred):**
- Short-lived JWT or opaque token issued after first auth.
- Token re-used for multiple frames in same logical session.
- Token expiry triggers re-authentication.
- Requires dedicated token store and revocation list.

---

## Audit Log Interface Contract

### EventEmitter Requirements

The EventEmitter (in andromeda-observe) must satisfy:

1. **Monotonic event IDs**: EventId increments without gaps.
2. **Validation before insertion**: All events validated; rejections counted.
3. **No silent drops**: If emission fails, error returned to caller.
4. **Immutability**: Stored events cannot be modified.
5. **Forensic traceability**: Every trace carries TraceId for correlation.

### AuditLog Interface (Future)

```rust
pub trait AuditLog: Send + Sync {
    /// Append an immutable SecurityAuditTrace.
    fn append(&mut self, trace: SecurityAuditTrace) -> AndromedaResult<()>;
    
    /// Read audit events in chronological order (range query).
    fn read_range(&self, start_time: SystemTime, end_time: SystemTime) 
        -> AndromedaResult<Vec<SecurityAuditTrace>>;
    
    /// Total number of allow decisions logged.
    fn allow_count(&self) -> u64;
    
    /// Total number of deny decisions logged.
    fn deny_count(&self) -> u64;
}
```

This interface is deferred to Wave 20 (persistent audit storage).

---

## No-Go Alignment

### Rule Compliance

**Rule: "No ad hoc SQL surface."**
- ✅ PASS: No Permission variant allows ad hoc SQL execution.
- ✅ PASS: SurfaceAction enum has no `RawSql` or `AdHocQuery` variant.

**Rule: "No security bypass through certificates."**
- ✅ PASS: Certificate scope validation enforced (DEC-018 + this decision).
- ✅ PASS: Principal binding immutable within session.
- ✅ PASS: No privilege escalation via certificate reuse across scopes.

**Rule: "Unrecoverable state changes."**
- ✅ PASS: Authorization failures are observable and logged.
- ✅ PASS: No action is silent; all decisions are audited.

**Rule: "Unbounded loops in SRPL core semantics."**
- ✅ PASS: Authorization checks do not execute user-provided code.
- ✅ PASS: Permission lookup is O(n) where n = number of roles (small, bounded).

---

## Implementation Contract (Wave 19, Design Only)

### Module: `andromeda-security`

**New crate location:** `crates/andromeda-security/`

**Module: `iam` (`src/iam/mod.rs`)**

Types to define:
- `UserPrincipal` (re-export from andromeda-observe)
- `CertificateIdentity` (re-export from andromeda-observe)
- `Permission` enum (extend if needed)
- `Role` enum (flat set of role names)
- `RolePermissionMatrix` (role → permissions mapping)
- `PrincipalRegistry` (certificate fingerprint → UserPrincipal lookup)
- `PermissionChecker` trait (permission validation logic)
- `IamContext` struct (session-scoped auth state)

**All APIs use `todo!()` — no implementation.**

**Tests:** Contract tests only (types compile, no logic tested yet).

---

## Testing and Validation Strategy

### Unit Tests (Wave 19)

- ✅ Permission enum is exhaustive (no missing variants)
- ✅ Role enum covers all standard roles
- ✅ RolePermissionMatrix is populated (all roles have permission sets)
- ✅ PrincipalRegistry lookups are deterministic

### Contract Tests (Wave 20)

- ✅ Authorization decision flow is correct
- ✅ Audit trace is always emitted (allow and deny)
- ✅ Default-deny is enforced (no permission → deny)
- ✅ Scope policy is enforced per listener

### Integration Tests (Wave 20+)

- ✅ Full mTLS → principal → permission → audit pipeline
- ✅ Certificate revocation closes connections
- ✅ Permission matrix is auditable
- ✅ Audit log is immutable

---

## Open Risks and Mitigations

### Risk 1: Certificate Expiry Not Checked in V0

**Risk:** An expired certificate is accepted if it's in PrincipalRegistry.  
**Impact:** Revoked/expired principals could still authenticate.  
**Mitigation:**
- Add certificate expiry check in Wave 20.
- For V0: Assume all certificates are valid (operational assumption).
- Document in release notes: "V0.5 does not validate certificate expiry."

### Risk 2: No Persistent Audit Log in V0

**Risk:** Audit events are lost on server shutdown.  
**Impact:** No forensic evidence after crash.  
**Mitigation:**
- Implement persistent audit storage in Wave 20.
- For V0: Acceptable for test/demo environments.
- Document: "Audit log is ephemeral; not suitable for production."

### Risk 3: Permission Revocation Requires Connection Close

**Risk:** Revoking a user's permission doesn't take effect until they reconnect.  
**Impact:** User could retain access briefly after revocation.  
**Mitigation:**
- Add connection-level revocation callback in Wave 21.
- For V0: Accept brief latency (reconnection-scoped).
- Document: "Permission changes effective on next connection."

### Risk 4: No Role Inheritance

**Risk:** Complex permission matrices for superadmin roles.  
**Impact:** Role definition duplication (all permissions listed for each role).  
**Mitigation:**
- Add role hierarchy in Wave 21 (parent roles, inheritance).
- For V0: Use flat roles with clear naming (e.g., `role_system_admin_full`).

### Risk 5: Fingerprint Collision (Cryptographic)

**Risk:** SHA256 collision (extremely unlikely, but non-zero).  
**Impact:** Two different certificates could be treated as identical.  
**Mitigation:**
- Use SHA256 (collision probability < 10^-77).
- For higher assurance: add full certificate DER encoding to audit trail.
- Document: "Fingerprint collisions are cryptographically infeasible."

### Risk 6: SurfaceScope Not Validated at Listener Boundary

**Risk:** A certificate valid for Application scope is presented to Administration listener.  
**Impact:** Cross-scope privilege escalation.  
**Mitigation:**
- **MUST** enforce in Wave 19: Listener validates certificate scope on accept.
- Per DEC-018: Each listener enforces its scope policy.
- Connection rejected if certificate scope doesn't match listener scope.

---

## Future Waves (Out of Scope)

### Wave 20 (V0.6): Persistent Audit Storage
- Implement AuditLog interface with durable storage.
- Add cryptographic signing of audit records.
- Add audit log read API (time range queries).

### Wave 21 (V0.7): Advanced IAM Features
- Role hierarchy and inheritance.
- Group expansion (user → groups → roles → permissions).
- Time-based permissions (e.g., "access allowed 9–5 Monday–Friday").
- Attribute-based access control (ABAC) for dynamic policies.

### Wave 22+ (V1.0): Compliance and Integration
- CRL/OCSP support for certificate revocation.
- Remote audit log replication.
- Tamper detection (immutable hash chain).
- Integration with external identity providers (LDAP, Kerberos, SAML).

---

## Change Impact

### Affected Modules

1. **andromeda-observe:**
   - Principal binding module already partially implemented.
   - Add SecurityAuditTrace emitter integration.
   - No breaking changes.

2. **andromeda-core:**
   - Add `IamError` and `IamErrorKind` variants.
   - No breaking changes to existing error handling.

3. **andromeda-quic:**
   - No changes. DEC-018 already defined contract.

4. **New: andromeda-security:**
   - Create crate with iam module.
   - Export types for use by dispatch layer.

### Dispatch Layer Integration (Future Wave)

The dispatch layer (andromeda-exec or runtime) will:
1. Extract certificate fingerprint from Connection.
2. Call `PrincipalRegistry.lookup(fingerprint)`.
3. Call `PermissionChecker.check(principal, permission)`.
4. Emit SecurityAuditTrace.
5. Proceed or reject based on outcome.

This integration is deferred to Wave 20 (dispatch layer refactor).

---

## Summary of Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Access model | RBAC (Role-Based) | Simpler than ABAC, easier to audit |
| Permission scope | Procedure-level | Matches execution model, reduces audit verbosity |
| Session lifetime | Request-scoped (no tokens) | Simpler for V0, certificate is identity |
| Default posture | Default-deny | Safer (no accidental grants) |
| Role hierarchy | Flat (no inheritance) | Simpler audit trail, prevent escalation |
| Audit immutability | Write-once, append-only | Forensic accountability |
| Scope validation | Per-listener | Prevent cross-scope privilege escalation |

---

## Validation Criteria (Wave 19)

- ✅ DEC-035 document complete (500+ lines)
- ✅ Type definitions created in andromeda-security crate
- ✅ All APIs are `todo!()` (no logic implemented)
- ✅ Audit trail specification clear
- ✅ Permission model specified
- ✅ Session semantics defined
- ✅ Default-deny principle enforced
- ✅ No-go rules compliance verified
- ✅ Integration points identified
- ✅ Open risks documented with mitigations

---

## Related Decisions

- **DEC-018:** mTLS Identity Extraction and Binding (prerequisite)
- **DEC-027:** HA/DR and Backup Audit Event Taxonomy (uses audit infrastructure)
- **DEC-014:** Rust Crate Module Structure (crate organization)

---

## Approval and Sign-Off

**Designed by:** Security IAM Auditor  
**Reviewed by:** [Project Lead]  
**Approved for Wave 19 Implementation:** [Timestamp]  

---

**End of DEC-035**
