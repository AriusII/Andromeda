#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Engine

The canonical system catalog, object version management, and contract tracking.

## Overview

The catalog module owns:
- **Object Definitions**: Tables, procedures, structured objects, enums
- **Object Versioning**: Catalog versions and compatibility tracking
- **Contract Hashes**: Stable hash computation for procedure contracts
- **Definition Batches**: Atomic multi-object updates to the catalog

## Core Concepts

### Catalog Objects

Objects in the catalog include:
- **Tables**: Named relations with columns
- **Procedures**: User-defined operations with input/output contracts
- **Structured Objects**: Named tuple types
- **Enums**: Named scalar types with discrete values

Each object has a unique `CatalogObjectId` and a version number that increments
when the object definition changes.

### Object References and Bindings

`CatalogObjectRef` provides a snapshot view of an object at a specific catalog version.
`CatalogObjectBinding` represents dependencies (e.g., a procedure that reads a table).

### Contracts

Procedure contracts define:
- Input and output column schemas
- Protocol layout (protobuf descriptor set)
- Transaction policies (isolation, access mode)
- Compatibility policies (AdditiveOnly, ExactHash)
- Error handling policies
- Result stream metadata

Contract hashes provide stable, deterministic fingerprints of procedure behavior,
enabling:
- Protocol version compatibility checking
- Contract evolution tracking
- Change impact analysis

### Definition Batches

Batch operations allow atomic application of multiple object definitions,
maintaining catalog consistency across related changes.

## Module Organization

| Module | Purpose |
|--------|---------|
| `objects` | Catalog object definitions and validation |
| `contracts` | Procedure contracts and compatibility checking |
| `names` | Qualified names and path resolution |
| `batch` | Transactional batch operations |
| `store` | Catalog storage interface |
| `snapshot` | Point-in-time catalog snapshots |
| `fixtures` | Test data and fixtures |

## Safety

This crate forbids unsafe code (`#![forbid(unsafe_code)]`).

"#]

mod batch;
mod contracts;
mod dependencies;
pub mod digest;
mod fixtures;
mod names;
mod objects;
mod plan_cache;
mod procedure_feedback;
mod procedure_store;
mod recovery;
mod scenario_evidence;
pub mod server;
mod snapshot;
mod statistics;
mod store;
mod wal_integration;
mod wal_record;

pub use batch::*;
pub use contracts::*;
pub use dependencies::*;
pub use fixtures::*;
pub use names::*;
pub use objects::*;
pub use plan_cache::*;
pub use procedure_feedback::*;
pub use procedure_store::*;
pub use recovery::*;
pub use scenario_evidence::*;
pub use server::*;
pub use snapshot::*;
pub use statistics::*;
pub use store::*;
pub use wal_integration::*;
pub use wal_record::*;
