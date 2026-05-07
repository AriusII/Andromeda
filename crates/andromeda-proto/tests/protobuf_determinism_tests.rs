#![forbid(unsafe_code)]

//! Protobuf determinism and schema compatibility validation tests.
//!
//! These tests ensure that:
//! - Serialization is deterministic (same message -> same bytes every time)
//! - Round-trip serialization preserves field values
//! - Edge cases (empty, zero, max values) are handled correctly
//! - Schema changes maintain wire compatibility

#[path = "support/proto_wire_fixtures.rs"]
pub(crate) mod proto_wire_fixtures;

#[path = "protobuf_determinism_tests/completion.rs"]
mod completion;
#[path = "protobuf_determinism_tests/determinism.rs"]
mod determinism;
#[path = "protobuf_determinism_tests/malformed.rs"]
mod malformed;
#[path = "protobuf_determinism_tests/manifest_resolution.rs"]
mod manifest_resolution;
#[path = "protobuf_determinism_tests/properties.rs"]
mod properties;
