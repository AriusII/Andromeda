#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Core

Foundation types and error handling for the Andromeda database engine.

## Overview

The core module provides:
- **Unique identifiers**: Request, session, transaction, catalog object, and database IDs
- **Error handling**: Structured error types with categories and recovery hints
- **Time management**: Clock abstractions for deterministic testing
- **Type system**: SQL type descriptors with validation rules
- **Hardware profiles**: CPU, RAM, and GPU capability detection and constraints
- **Resource budgets**: Memory and stream limits for execution

## Core Types

### Identifiers

All ID types wrap `u64` for stability and range representation:
- `RequestId`: Correlates related protocol messages
- `SessionId`: Groups requests from a single client connection
- `TransactionId`: Unique transaction reference
- `CatalogObjectId`: References objects in the system catalog
- `CatalogVersion`: Increments when catalog objects change

### Error Handling

Errors are categorized by `AndromedaErrorKind` to enable:
- Client-friendly error classification
- Automated retry policies
- Monitoring and metrics
- Transaction recovery strategies

### Time Management

`Clock` trait abstracts time for:
- Deterministic distributed execution
- Testing without real-world delays
- Timestamp consistency across replicas

### Type System

`TypeDescriptor` combines a `ScalarType` with an `AbsencePolicy` to describe
SQL value semantics:
- Numeric types: I8..I128, U8..U128, Decimal, Float
- String types: Text with encoding and length constraints
- Temporal types: Transaction, Invocation, or Monotonic timestamps
- Boolean: True/false values
- Absence: Required or ExplicitOptional

Validation rules prevent:
- Invalid decimal precision/scale combinations
- Text types with zero max length
- Float types used as exact relational invariants

### Hardware Profiles

Hardware modules provide:
- **CPU**: Architecture detection and SIMD capability classification
- **RAM**: Memory budget allocation by functional role
- **GPU**: Availability and execution policy constraints
- **Pipeline**: Operation classification (commit, rollback, analytics, etc.)

A complete `HardwareProfile` aggregates all capabilities for:
- Resource scheduling decisions
- Execution strategy selection
- Constraint validation during execution

## Modules

| Module | Purpose |
|--------|---------|
| `error` | Error types and categorization |
| `ids` | Unique identifier types |
| `time` | Clock abstractions and timestamps |
| `types` | SQL type system and descriptors |
| `hardware_cpu` | CPU capability profiles |
| `hardware_ram` | RAM budget allocation |
| `hardware_gpu` | GPU execution policies |
| `hardware_pipeline` | Pipeline operation classification |
| `hardware_integration` | Aggregated hardware profile |

## Safety

This crate forbids unsafe code (`#![forbid(unsafe_code)]`).

## Examples

### Error Handling

```ignore
use andromeda_core::{AndromedaError, AndromedaErrorKind};

let err = AndromedaError::new(
    AndromedaErrorKind::Contract,
    "procedure contract mismatch",
);
```

### Hardware Profile

```ignore
use andromeda_core::{HardwareProfile, PipelineClass};

let profile = HardwareProfile::conservative();
profile.validate_gpu_pipeline(PipelineClass::BatchAnalytics)?;
```

### Type System

```ignore
use andromeda_core::{ScalarType, TypeDescriptor, AbsencePolicy};

let col_type = TypeDescriptor::optional(ScalarType::I64);
col_type.validate()?;
```

"#]

// Core infrastructure modules
pub mod digest;
mod error;
mod ids;
mod time;
mod types;

// IAM and security modules (Wave 19+)
pub mod principal;

// Hardware capability modules (modular hardware definition)
mod hardware_cpu;
mod hardware_gpu;
mod hardware_integration;
mod hardware_pipeline;
mod hardware_ram;

// Re-export core error and id types
pub use error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use ids::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, InvocationId, NamespaceId,
    ProcedureId, RequestId, SessionId, TransactionId,
};

// Re-export IAM types
pub use principal::{
    CertificateFingerprint, Permission, PermissionSet, Principal, PrincipalId, PrincipalRole,
    SessionToken,
};

// Re-export hardware capability types (CPU)
pub use hardware_cpu::{CpuCapabilityClass, CpuProfile, HardwareArchitecture};

// Re-export hardware capability types (GPU)
pub use hardware_gpu::{GpuExecutionPolicy, GpuProfile};

// Re-export hardware capability types (Pipeline)
pub use hardware_pipeline::PipelineClass;

// Re-export hardware capability types (RAM)
pub use hardware_ram::{RamProfile, RamSectionBudget, RamSectionRole};

// Re-export integrated hardware profile and resource budget
pub use hardware_integration::{HardwareProfile, ResourceBudget};

// Re-export time types
pub use time::{Clock, EngineTimestamp, ManualClock, SystemClock};

// Re-export type system types
pub use types::{
    AbsencePolicy, ColumnDescriptor, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding,
    TextType, TimestampType, TypeDescriptor,
};
