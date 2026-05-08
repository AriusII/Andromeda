//! Compatibility facade for HADR ownership.
//!
//! HADR membership, quorum, shipping, and promotion decision logic now lives in
//! `andromeda-hadr`. Storage keeps this module so existing
//! `andromeda_storage::hadr` imports continue to resolve during crate
//! extraction.

pub use andromeda_hadr::*;
pub use andromeda_hadr::{membership_transitions, quorum_runtime, shipping_runtime};
