//! Storage compatibility surface for immutable cold segment publication.
//!
//! Canonical owner: `andromeda_manifest`. Storage keeps this re-export because
//! active storage tests still import `PublishedColdSegment` from the storage
//! crate root.

pub use andromeda_manifest::PublishedColdSegment;
