//! Snapshot and manifest publication contracts.
//!
//! # Canonical ownership
//!
//! All publication contract types and free functions are owned by the root
//! module `crate::manifest` (re-exported at the crate root via
//! `pub use manifest::*`). This `publication` module is a **documented
//! facade** that groups the publication-domain contracts under a stable,
//! discoverable path for callers, integration tests, and downstream crates.
//!
//! Every item exported here is a `pub use` re-export of a `crate::manifest`
//! definition. This module MUST NOT introduce new `struct`, `enum`, `trait`,
//! `fn`, or `const` definitions. Adding a duplicate definition would break
//! the facade-identity regression tests in
//! `tests/publication_facade_invariants.rs`, which assert that every facade
//! type resolves to the same `TypeId` as its canonical root path and can be
//! used interchangeably (function-pointer identity for the boundary
//! validator, byte-for-byte equality for value types).
//!
//! # Doctrinal scope
//!
//! These contracts encode the cold-snapshot + WAL publication boundary that
//! makes ColdStore + durable WAL the only sources of truth. They MUST NOT
//! widen to expose RAM-resident or hot-only state; new publication-domain
//! contracts must first be defined in `crate::manifest` and only then
//! surfaced here.
//!
//! Canonical mappings:
//!
//! | Facade path                                                  | Canonical owner                                       |
//! |--------------------------------------------------------------|-------------------------------------------------------|
//! | `publication::DatabaseManifest`                              | `crate::manifest::DatabaseManifest`                   |
//! | `publication::SnapshotSegmentReference`                      | `crate::manifest::SnapshotSegmentReference`           |
//! | `publication::DatabaseSnapshotPublication`                   | `crate::manifest::DatabaseSnapshotPublication`        |
//! | `publication::ColdSegmentPublicationPlan`                    | `crate::manifest::ColdSegmentPublicationPlan`         |
//! | `publication::SnapshotAvailabilityContract`                  | `crate::manifest::SnapshotAvailabilityContract`       |
//! | `publication::validate_cold_segment_publication_boundary`    | `crate::manifest::validate_cold_segment_publication_boundary` |

pub use crate::{
    ColdSegmentPublicationPlan, DatabaseManifest, DatabaseSnapshotPublication,
    SnapshotAvailabilityContract, SnapshotSegmentReference,
    validate_cold_segment_publication_boundary,
};
