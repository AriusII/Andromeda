# JSON_SCHEMA — Output Contract Schema

**Version:** 1.0.0  
**Updated:** Q1 2026  
**Language:** American English  
**Style:** Microsoft Documentation  

---

## Overview

This document specifies the JSON schema for Andromeda Procedure result serialization. JSON output is produced for **diagnostics only** and is never used as a runtime mutation path or the normative wire format. The normative wire format remains **Protobuf + QUIC**.

The StructuredObject contract (defined in `crates/andromeda-proto/src/structured.rs`) provides the typed tabular schema for Procedure results. This document describes how StructuredObject instances are serialized to JSON for observability, logging, and evidence archival.

---

## StructuredObject Contract

### Purpose

A StructuredObject is the only transactional result shape callable by RPC. It packages:

1. **Metadata:** Name, descriptor hash, field definitions, row count policy.
2. **Payload:** Binary-encoded rows (row-major, column-major, or hybrid layout).
3. **Bounds:** Declared row count, payload size limits, checksums.

The metadata-before-payload contract ensures all downstream framers (RPC batch, manifest emission, result streaming) can validate envelope integrity before reading payload bytes.

### StructuredObjectHeader Structure

Every StructuredObject begins with a header containing the complete contract definition:

```rust
pub struct StructuredObjectHeader {
    pub name: String,                      // Non-empty identifier
    pub contract_hash: ContractHash,       // 32-byte fingerprint of owning procedure
    pub descriptor_hash: ContractHash,     // 32-byte fingerprint of field set + layout
    pub fields: Vec<ColumnDescriptor>,     // Ordered field definitions
    pub column_count: u32,                 // Must equal fields.len()
    pub layout: StructuredObjectLayout,    // RowMajor | ColumnMajor | Hybrid
    pub row_count_policy: RowCountPolicy,  // ExactRequired | Bounded | Unbounded
    pub row_count_exact: Option<u64>,      // Row count if policy requires or provides it
    pub payload_length: u64,               // Declared payload byte count
    pub payload_checksum: Option<u64>,     // Optional content digest
    pub max_payload_length: Option<u64>,   // Reject oversized payloads
}
```

---

## JSON Serialization Schema

### Top-Level Result Envelope

```json
{
  "_envelope": {
    "timestamp": "2026-01-15T14:30:45Z",
    "source": "<source-origin>",
    "version": "1.0.0",
    "diagnostic_only": true
  },
  "structured_object": {
    "header": { <see StructuredObjectHeader below> },
    "rows": [ <see Row below> ]
  }
}
```

### Field Definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `timestamp` | ISO 8601 | Yes | UTC timestamp of serialization. |
| `source` | String | Yes | Origin (e.g., "andromeda-cli", "hadr-status-inspector", "benchmark-runner"). |
| `version` | String | Yes | Schema version (e.g., "1.0.0"). |
| `diagnostic_only` | Boolean | Yes | Must always be `true`; indicates this is not a normative wire format. |
| `structured_object` | Object | Yes | Container for header and rows. |

### StructuredObjectHeader Serialization

```json
{
  "header": {
    "name": "Inventory.ReserveStock.Result",
    "contract_hash": "<hex-encoded 32-byte sha256>",
    "descriptor_hash": "<hex-encoded 32-byte sha256>",
    "columns": [
      {
        "ordinal": 0,
        "name": "success",
        "scalar_type": "Bool",
        "nullable": false
      },
      {
        "ordinal": 1,
        "name": "reserved_quantity",
        "scalar_type": "Int32",
        "nullable": false
      },
      {
        "ordinal": 2,
        "name": "message",
        "scalar_type": "Text",
        "nullable": true
      }
    ],
    "column_count": 3,
    "layout": "RowMajor",
    "row_count_policy": "ExactRequired",
    "row_count_exact": 5,
    "payload_length": 1024,
    "payload_checksum": "<hex-encoded u64>",
    "max_payload_length": 65536
  }
}
```

### ColumnDescriptor Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `ordinal` | u32 | Yes | Zero-based column index (must be dense). |
| `name` | String | Yes | Column identifier (must be unique within header). |
| `scalar_type` | String | Yes | Data type (Bool, Int32, Int64, Decimal, Text, Bytes, Timestamp, Float, etc.). |
| `nullable` | Boolean | Yes | Whether null values are permitted for this column. |
| `encoding` | String | No | Text encoding for Text fields ("UTF8", "ASCII"), or decimal precision for Decimal. |
| `default_value` | Any | No | Default value if not present (diagnostic only). |

### Supported Scalar Types

| Type | JSON Format | Range / Constraints |
|------|-------------|-------------------|
| `Bool` | `true` \| `false` | Boolean literal. |
| `Int32` | Integer | -2,147,483,648 to 2,147,483,647 |
| `Int64` | Integer or String | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 (use string for JSON safety) |
| `Decimal` | String | "123.45" or "1e-10" (scientific notation); preserves precision. |
| `Float` | Number | IEEE 754 double precision. Special values: `null` for NaN, infinity. |
| `FloatMode` | String | "Strict" (no approximation) or "Approximate" (lossy rounding). |
| `Text` | String | UTF-8 encoded; empty string allowed. |
| `Bytes` | String | Base64-encoded payload; no padding required. |
| `Timestamp` | String | ISO 8601 format: "2026-01-15T14:30:45.123456Z". |
| `TextEncoding` | String | "UTF8", "ASCII", or codec name. |
| `TimestampType` | String | "UtcNanosecond", "UtcMillisecond", or format specifier. |

### Row Serialization

Rows are serialized as ordered arrays of column values (row-major order):

```json
{
  "rows": [
    {
      "0": true,                          // Column 0: success (Bool)
      "1": 50,                            // Column 1: reserved_quantity (Int32)
      "2": "Order fulfillable"            // Column 2: message (Text, nullable)
    },
    {
      "0": false,
      "1": null,
      "2": "Insufficient stock"
    }
  ]
}
```

**Alternative Row Format (Named Columns):**

```json
{
  "rows": [
    {
      "success": true,
      "reserved_quantity": 50,
      "message": "Order fulfillable"
    },
    {
      "success": false,
      "reserved_quantity": null,
      "message": "Insufficient stock"
    }
  ]
}
```

**Row Encoding Rules:**

- Null values are represented as `null` (JSON null literal).
- Numeric columns that exceed JSON integer range (> 2^53) must be encoded as strings.
- Bytes are Base64-encoded without padding; consumers must pad before decoding.
- Timestamps are always ISO 8601 UTC with nanosecond precision (trailing zeros allowed).

---

## Example: Inventory.ReserveStock Result

**Procedure Contract:** `Inventory.ReserveStock(stock_id: Text, quantity: Int32) -> StructuredObject`

**Result Structure:**

```json
{
  "_envelope": {
    "timestamp": "2026-01-15T14:30:45.123456Z",
    "source": "andromeda-cli",
    "version": "1.0.0",
    "diagnostic_only": true
  },
  "structured_object": {
    "header": {
      "name": "Inventory.ReserveStock.Result",
      "contract_hash": "a3f4b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9",
      "descriptor_hash": "f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0",
      "columns": [
        {
          "ordinal": 0,
          "name": "success",
          "scalar_type": "Bool",
          "nullable": false
        },
        {
          "ordinal": 1,
          "name": "reserved_quantity",
          "scalar_type": "Int32",
          "nullable": false
        },
        {
          "ordinal": 2,
          "name": "message",
          "scalar_type": "Text",
          "nullable": true,
          "encoding": "UTF8"
        }
      ],
      "column_count": 3,
      "layout": "RowMajor",
      "row_count_policy": "ExactRequired",
      "row_count_exact": 1,
      "payload_length": 256,
      "payload_checksum": "0x123456789abcdef0",
      "max_payload_length": 65536
    },
    "rows": [
      {
        "success": true,
        "reserved_quantity": 50,
        "message": "Reservation successful; 50 units reserved"
      }
    ]
  }
}
```

---

## Error Result Shape

When a Procedure fails, the result envelope includes error metadata:

```json
{
  "_envelope": {
    "timestamp": "2026-01-15T14:30:45Z",
    "source": "andromeda-cli",
    "version": "1.0.0",
    "diagnostic_only": true
  },
  "error": {
    "code": "InsufficientStock",
    "kind": "ProcedureError",
    "message": "Cannot reserve 100 units; only 25 available",
    "context": {
      "stock_id": "WIDGET-SKU-001",
      "requested_quantity": 100,
      "available_quantity": 25
    }
  }
}
```

**Error Fields:**

| Field | Type | Meaning |
|-------|------|---------|
| `code` | String | Machine-readable error code (e.g., "InsufficientStock", "ContractViolation"). |
| `kind` | String | Error classification (ProcedureError, ContractError, AuthorizationError, TransactionError). |
| `message` | String | Human-readable description. |
| `context` | Object | Diagnostic context (Procedure parameters, LSN, epoch, etc.). |
| `stack_trace` | String | [Optional] Debug stack trace (diagnostic environments only). |

---

## Validation Rules

### Metadata-Before-Payload Invariants

Every StructuredObject must satisfy these invariants before the payload is accepted:

1. **Non-empty name:** `name.trim().len() > 0`
2. **Non-zero contract hash:** `contract_hash != 0x00000...`
3. **Non-zero descriptor hash:** `descriptor_hash != 0x00000...`
4. **At least one field:** `fields.len() > 0`
5. **Column count consistency:** `column_count == fields.len()`
6. **Dense ordinals:** `fields[i].ordinal == i` for all i in 0..fields.len()
7. **Unique field names:** No duplicate names within fields.
8. **Descriptor hash match:** Recomputed descriptor hash equals declared descriptor_hash.
9. **Row count coherence:**
   - If `row_count_policy == ExactRequired`: `row_count_exact` must be Some.
   - If `row_count_exact == 0`: `payload_length` must be 0.
   - If `row_count_exact > 0`: `payload_length` must be > 0.
10. **Payload bounds:** If `max_payload_length` is set, `payload_length <= max_payload_length`.

### Validation Failure Behavior

Framers (RPC handlers, manifest emitters, recovery processes) must:

- **Reject on validation failure:** Do not admit payloads with invalid headers.
- **Fail fast:** Validate before streaming payload bytes.
- **Emit audit event:** Log validation failure to the audit ledger (DEC-033).
- **Return error:** Report specific validation error to caller with error code.

---

## Row Count Policy

The `row_count_policy` determines how row counts are advertised and validated.

### Policy Types

| Policy | Meaning | Requirement |
|--------|---------|-------------|
| `ExactRequired` | Exact row count is mandatory and must match payload. | `row_count_exact` must be Some; validation ensures row count matches. |
| `Bounded` | Row count is bounded by `row_count_exact`; actual count may be less. | `row_count_exact` is upper bound; actual rows <= bound. |
| `Unbounded` | Row count is not constrained by metadata. | `row_count_exact` is None; payload size is the only constraint. |

### Contract Invariant

The **RowCountExact** invariant states: If a StructuredObject contract declares `row_count_policy == ExactRequired`, the row count is verifiable purely from metadata (not from a scan of the payload).

This allows result-stream framers and recovery processes to reject non-conformant payloads without reading payload bytes.

---

## Version Compatibility

### Schema Evolution

The JSON schema is versioned. Compatibility rules:

1. **Additions:** New optional fields in the schema are backward compatible.
2. **Removals:** Removing required fields breaks compatibility; requires major version bump.
3. **Renames:** Renaming fields breaks compatibility; requires major version bump.
4. **Type changes:** Changing the JSON type of a field (e.g., String to Number) breaks compatibility.

### Handling Future Versions

Consumers of JSON envelopes should:

1. Check `version` field in `_envelope`.
2. If version is newer than supported, emit a warning but continue processing.
3. Ignore unknown fields gracefully.
4. Do not rely on field order or assume compact representation.

### Current Version

- **Version:** 1.0.0
- **Release:** Q1 2026
- **Stability:** Locked for Wave 12 Phase H

---

## Layout Variants

The `layout` field describes the physical encoding of rows in the payload.

### RowMajor

**Encoding:** Rows stored consecutively; each row contains all columns.

**Advantages:** Simple streaming, suitable for result transmission.

**Example (pseudo-bytes):**

```text
Row 0: [col0_bytes][col1_bytes][col2_bytes]
Row 1: [col0_bytes][col1_bytes][col2_bytes]
```

### ColumnMajor

**Encoding:** Columns stored consecutively; each column contains all rows.

**Advantages:** Efficient for column-oriented processing, cache-friendly for analytics.

**Example (pseudo-bytes):**

```text
Column 0: [row0_col0][row1_col0][...]
Column 1: [row0_col1][row1_col1][...]
Column 2: [row0_col2][row1_col2][...]
```

### Hybrid

**Encoding:** Mixed row-major and column-major regions (implementation-specific).

**Advantages:** Balances streaming and analytics efficiency.

**Example:** First 1000 rows stored row-major; remaining rows stored column-major.

---

## Checksum and Integrity

### Payload Checksum

If `payload_checksum` is present, it contains a digest of the payload bytes:

- **Format:** Hex-encoded u64 (8 bytes).
- **Algorithm:** [Implementation-defined; document in runtime-specific schema].
- **Validation:** Framers should recompute checksum and compare on receipt.
- **Failure:** Checksum mismatch indicates payload corruption; reject and log audit event.

### Example with Checksum

```json
{
  "header": {
    "name": "Inventory.ReserveStock.Result",
    "contract_hash": "a3f4b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9",
    "descriptor_hash": "f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0",
    "columns": [ ... ],
    "row_count_exact": 100,
    "payload_length": 4096,
    "payload_checksum": "0xcafebabe12345678",
    "max_payload_length": 65536
  }
}
```

---

## Related Documentation

- **DEC-021:** Protobuf Schema Contract (normative wire format)
- **DEC-033:** Durable Audit Ledger (evidence archival)
- **CLI.md:** Command output examples
- **BENCHMARK.md:** Benchmark evidence format
- **structured.rs source:** Implementation reference

---

## Glossary

| Term | Meaning |
|------|---------|
| **StructuredObject** | Typed tabular result shape; only form returned by Procedures. |
| **Envelope** | Top-level JSON container with metadata and error handling. |
| **Header** | Metadata-before-payload contract describing schema, bounds, and hashes. |
| **Descriptor** | Field set and layout definition; identified by deterministic hash. |
| **Contract Hash** | 32-byte SHA256 fingerprint of Procedure definition or result schema. |
| **Descriptor Hash** | 32-byte SHA256 fingerprint of field set and layout (name-independent). |
| **Row Count Policy** | Rule governing how row counts are advertised (ExactRequired, Bounded, Unbounded). |
| **Payload** | Binary-encoded rows; size and checksum declared in header. |
| **Layout** | Physical row encoding (RowMajor, ColumnMajor, Hybrid). |
| **Diagnostic Only** | JSON output never used as normative wire format; Protobuf is normative. |

