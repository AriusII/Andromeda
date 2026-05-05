# DEC-022: Protocol Stability and Drift Detection

**Status:** Accepted (D7 - Protocol Drift Scans)  
**Date:** 2025 (Andromeda V0.5)  
**Authors:** QUIC Transport Architect  
**Impact:** Frame encoding/decoding, Protobuf contract, RPC dispatch  

---

## Problem Statement

Andromeda's wire protocol is immutable for V0.5. Frame format, Protobuf schema, and RPC discriminators must never change—even accidentally. However, there is currently no automated detection system to catch protocol drift caused by:

1. **Accidental field reordering** in FrameHeader struct
2. **Discriminator mutations** in FrameType or PayloadKind enums
3. **Protobuf schema changes** (field reordering, type mutations)
4. **Version number drift** (ProtocolVersion no longer locked at V1.0)
5. **Regression regressions** between commits (frame type codes shift)

Without proactive detection, protocol violations may not be caught until testing or production, causing silent wire-level incompatibilities.

---

## Design Principles

### 1. Defense in Depth

**Compile-time + runtime checks** provide layered protection:

- **Compile-time:** Struct layout assertions ensure FrameHeader stays 52 bytes
- **Runtime:** Module initialization validates all invariants before any connection

### 2. Fail-Fast on Protocol Drift

Any invariant violation immediately fails with `AndromedaError::Protocol`:

```rust
if FrameHeader size has changed {
    return Err("frame header layout drift detected")
}
```

### 3. Explicit Invariant Declaration

All protocol constraints are written as explicit values:

- Frame type codes: `HELLO_FRAME_CODE = 1`, etc.
- Payload kinds: `PayloadKind::Hello = 1`, etc.
- Protocol version: `ProtocolVersion::V1 = (1, 0)`

### 4. No Ad Hoc Serialization

Protocol invariants are NOT derived from runtime behavior. They are locked constants validated against wire definitions.

---

## Architecture

### Module: `crates/andromeda-quic/src/protocol_invariants.rs`

Provides three layers of validation:

#### Layer 1: Frame Header Layout (`FrameHeader`)

```rust
pub struct FrameHeader {
    pub frame_type: FrameType,          // Offset 0, 4 bytes
    pub request_id: RequestId,          // Offset 4, 8 bytes
    pub session_id: SessionId,          // Offset 12, 8 bytes
    pub tx_id: Option<TransactionId>,   // Offset 20, 16 bytes
    pub payload_length: u64,            // Offset 36, 8 bytes
    pub flags: u32,                     // Offset 44, 4 bytes
    pub header_crc: u32,                // Offset 48, 4 bytes
}
// Total: 52 bytes (immutable)
```

**Invariants:**
- Size = 52 bytes
- CRC at offset 48 (last 4 bytes)
- Field ordering preserved
- No field insertion/deletion

**Detection:**
- `mem::size_of::<FrameHeader>() == 52`
- `validate_frame_header_layout()` runtime check
- Compile-time assertions via const functions

#### Layer 2: Frame Type Discriminators (`FrameTypeInvariants`)

```rust
pub const HELLO_FRAME_CODE: u32 = 1;
pub const AUTH_FRAME_CODE: u32 = 2;
pub const CONTRACT_REQUEST_FRAME_CODE: u32 = 3;
pub const CONTRACT_RESPONSE_FRAME_CODE: u32 = 4;
pub const RPC_EXECUTE_REQUEST_FRAME_CODE: u32 = 5;
pub const RPC_METADATA_FRAME_CODE: u32 = 6;
pub const RPC_BATCH_FRAME_CODE: u32 = 7;
pub const RPC_COMPLETION_FRAME_CODE: u32 = 8;
pub const ERROR_FRAME_CODE: u32 = 9;
pub const TELEMETRY_SOFT_SIGNAL_FRAME_CODE: u32 = 100;
```

**Invariants:**
- Each code is immutable
- Codes 1-9 reserved for transport frames
- Code 100 reserved for telemetry
- No new codes in range [1..99]

**Detection:**
- `FrameTypeInvariants::validate()` checks each code
- `FrameType::try_from(code)` must map correctly
- `FrameType::wire_code()` returns expected value

#### Layer 3: Payload Kind Discriminators (`PayloadKindInvariants`)

```rust
pub enum PayloadKind {
    Hello = 1,
    Auth = 2,
    ContractRequest = 3,
    ContractResponse = 4,
    RpcExecuteRequest = 5,
    RpcMetadata = 6,
    RpcBatch = 7,
    RpcCompletion = 8,
    Error = 9,
}
```

**Invariants:**
- All discriminators in range [1..8] (9 is Error)
- 1:1 mapping to FrameType codes (transport codes)
- Lock payload-to-frame-type binding

**Detection:**
- `PayloadKindInvariants::validate()` checks each code
- `PayloadKind::try_from(code)` must map correctly
- `PayloadKind::wire_code()` returns expected value

#### Layer 4: Protocol Version Lock (`ProtocolVersionInvariants`)

```rust
pub struct ProtocolVersion {
    pub major: u32,  // = 1 (locked)
    pub minor: u32,  // = 0 (locked)
}
```

**Invariants:**
- V1.0 is the only supported version
- `ProtocolVersion::SUPPORTED_MAJOR = 1`
- `ProtocolVersion::SUPPORTED_MINOR = 0`
- Future versions (V1.1, V2.0) are rejected

**Detection:**
- `ProtocolVersionInvariants::validate()` checks version
- `ProtocolVersion::validate()` rejects non-V1.0
- Master validation on connection init

#### Layer 5: Master Validation (`ProtocolInvariants`)

Orchestrates all checks:

```rust
pub fn ProtocolInvariants::validate_all() -> AndromedaResult<()> {
    validate_frame_header_layout()?;
    FrameTypeInvariants::validate()?;
    PayloadKindInvariants::validate()?;
    ProtocolVersionInvariants::validate()?;
    Ok(())
}
```

Should be called:
- At crate initialization
- Before accepting first connection frame
- During handshake validation

---

## Validation Strategy

### Compile-Time Validation

```rust
// In protocol_invariants.rs
pub const fn assert_frame_header_layout() {
    const _: () = const {
        assert!(
            mem::size_of::<FrameHeader>() == 52,
            "FrameHeader size changed!"
        );
    };
}
```

If FrameHeader struct size changes, compilation will fail at const evaluation time, before any code runs.

### Runtime Validation

```rust
// Called during module initialization
pub fn validate_frame_header_layout() -> AndromedaResult<()> {
    if mem::size_of::<FrameHeader>() != 52 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame header layout drift detected",
        ));
    }
    Ok(())
}
```

Catches runtime deviations that compile-time checks missed.

### Test Coverage

12 comprehensive tests in `protocol_stability_contract.rs`:

1. **test_frame_header_offset_stability** — Size = 52 bytes
2. **test_payload_kind_enum_values_locked** — Codes [1..8, 9]
3. **test_protobuf_field_ordering_stable** — Descriptor hashes deterministic
4. **test_protocol_version_locked_at_1_0** — V1.0 enforced
5. **test_frame_type_discriminator_unchanged** — Codes [1-8, 100]
6. **test_crc_position_stable** — Offset 48 immutable
7. **test_frame_payload_max_size_locked** — 16 MiB lock
8. **test_regression_detect_field_reordering** — Layout drift detection
9. **test_master_protocol_invariant_validation** — All checks pass
10. **test_payload_to_frame_type_mapping_consistency** — Synchronization
11. **test_frame_family_mapping_preserved** — Family routing stable
12. **test_payload_requirements_preserved** — Empty payload/contract hash rules

---

## Regression Detection Policy

### What We Detect

✅ **Struct field reordering** — Size mismatch, offset changes  
✅ **Discriminator mutation** — Code value changes  
✅ **Version drift** — ProtocolVersion no longer V1.0  
✅ **Field insertion/deletion** — Size grows/shrinks  
✅ **Protobuf schema changes** — Descriptor hash shifts  
✅ **Transport mapping break** — PayloadKind ↔ FrameType sync lost  

### What We Don't Detect (Acknowledged Gaps)

❌ **Unused field addition** (if size stays 52) — Would need per-field inspection  
❌ **Field rename** (if binary representation unchanged) — Would need symbol inspection  
❌ **Payload format change inside struct** (if wire codes unchanged) — Would need semantic analysis  

**Mitigation:** These gaps are acceptable because:
1. Field-level changes require careful review
2. Code review + pull request discipline catches these
3. Deep semantic changes are caught by integration tests

---

## Integration Points

### D6 Protobuf Schema Contract

- **Input:** Descriptor set hash from `andromeda_proto`
- **Validation:** `descriptor_set_hash()` must be deterministic
- **Output:** On drift, fail with `AndromedaError::Protocol`

### Frame Encoding/Decoding (Frame Layer)

- **Input:** Invariant validation must pass before codec usage
- **Output:** Codec can assume FrameHeader is always 52 bytes
- **Benefit:** Eliminates size checks in hot path

### RPC Dispatch (D1)

- **Input:** PayloadKind discriminators must be stable
- **Output:** Dispatch router can use switch statements safely
- **Guarantee:** No payload code reuse across frame types

### Stream Concurrency (D5)

- **Input:** FrameType frame families stable
- **Output:** Stream role enforcement unchanged
- **Benefit:** No need to re-validate frame family on each frame

---

## Activation

### When to Call Validation

**Option 1: Early Validation (Recommended)**

```rust
// In andromeda_quic lib.rs or main()
#[ctor::ctor]
fn init_protocol_invariants() {
    if let Err(e) = ProtocolInvariants::validate_all() {
        panic!("Protocol drift detected on startup: {}", e);
    }
}
```

**Option 2: Connection-Time Validation**

```rust
// In Connection::new()
pub fn new(...) -> AndromedaResult<Self> {
    ProtocolInvariants::validate_all()?;
    // ... rest of initialization
}
```

**Option 3: Lazy Validation**

```rust
// In FrameCodec::decode() or similar hot path
lazy_static! {
    static ref PROTOCOL_OK: () = {
        ProtocolInvariants::validate_all().expect("protocol drift");
    };
}
// Use PROTOCOL_OK on first codec operation
```

---

## Version Evolution Policy

### Current State: V1.0 Locked

All Andromeda V0.5 deployments use **ProtocolVersion V1.0** (major=1, minor=0).

### Future: Major Version Bump (V2.0)

If breaking wire format changes are needed:

```
1. Create ProtocolVersion V2.0 in code
2. Update SUPPORTED_MAJOR = 2 (or add version enum)
3. Add version negotiation in Hello/Auth frames
4. Maintain backward compatibility with legacy clients (accept V1.0)
5. Update this decision record
```

### Future: Minor Version Bump (V1.1)

If backward-compatible extensions are needed:

```
1. Add optional field to existing message
2. Update SUPPORTED_MINOR = 1
3. V1.0 clients ignore new field (proto3 default)
4. V1.1 servers accept V1.0 clients (backward compatible)
5. Update this decision record
```

---

## Testing and Validation

### Test Matrix

| Test | Validates | Frequency | Failure Mode |
|------|-----------|-----------|--------------|
| `test_frame_header_offset_stability` | Size = 52 | Every build | Panic on test |
| `test_payload_kind_enum_values_locked` | Codes 1-8, 9 | Every build | Panic on test |
| `test_protocol_version_locked_at_1_0` | V1.0 only | Every build | Panic on test |
| `test_frame_type_discriminator_unchanged` | Codes 1-8, 100 | Every build | Panic on test |
| `test_crc_position_stable` | Offset 48 | Every build | Panic on test |
| Master validation | All checks | Connection init | `AndromedaError::Protocol` |

### Continuous Integration

Add to CI pipeline:

```yaml
- name: Protocol Stability Tests
  run: cargo test --test protocol_stability_contract
```

### Diagnostic Report

On drift detection, generate diagnostic:

```rust
println!("{}", ProtocolInvariants::diagnostic_report());
```

Outputs:

```
=== Protocol Invariants Diagnostic Report ===

Frame Header Layout:
  Size: 52 bytes (expected 52)
  Frame Type Code Invariants: 10 items locked

Frame Type Codes:
  HELLO: 1
  AUTH: 2
  ...
```

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| **False positives in CI** (e.g., platform-specific struct size) | Explicit size checks + cross-platform testing |
| **Validation overhead** (runtime cost) | Lazy validation + caching (single-call per lifetime) |
| **Test brittleness** (code changes without protocol changes) | Tests are *only* for invariants, not implementation |
| **Undetected enum reordering** (if only repr changes) | Code review discipline + integration tests |
| **Version 2.0 complexity** | Document upgrade path clearly in decision record |

---

## References

- **D1 RPC Dispatch:** Relies on PayloadKind stability
- **D5 Stream Concurrency:** Relies on FrameType family stability
- **D6 Protobuf Schema Contract:** Provides descriptor hash
- **Protocol Module** (`andromeda_proto`): Provides ProtocolVersion validation

---

## Approval

- ✅ QUIC Transport Architect: Protocol drift detection approved
- ✅ Frame layer integration: Compile-time assertions verified
- ✅ RPC dispatch integration: Discriminator locking verified
- ✅ Test suite: 12 comprehensive tests covering all invariants

---

## Summary

**Protocol Drift Scans (D7)** provides layered defense against wire protocol mutations:

1. **Compile-time assertions** catch struct layout changes at build time
2. **Runtime validation** catches version and code mutations at startup
3. **Comprehensive test suite** provides regression detection
4. **Explicit invariant declarations** make protocol constraints visible
5. **Master validator** orchestrates all checks for fail-fast behavior

This ensures Andromeda's V1.0 protocol remains stable, interoperable, and debuggable across all deployments.
