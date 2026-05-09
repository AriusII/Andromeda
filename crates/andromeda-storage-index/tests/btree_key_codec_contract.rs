//! B-tree key codec contract tests.
//!
//! Focused modules validate order-preserving key encoding/decoding,
//! lexicographic comparison, deterministic durable bytes, and composite
//! schema validation for B-Tree indexing.

#![forbid(unsafe_code)]

#[path = "btree_key_codec_contract/comparator_format.rs"]
mod comparator_format;
#[path = "btree_key_codec_contract/composite_schema.rs"]
mod composite_schema;
#[path = "btree_key_codec_contract/determinism.rs"]
mod determinism;
#[path = "btree_key_codec_contract/golden.rs"]
mod golden;
#[path = "btree_key_codec_contract/ordering.rs"]
mod ordering;
#[path = "btree_key_codec_contract/roundtrip.rs"]
mod roundtrip;
#[path = "btree_key_codec_contract/support.rs"]
mod support;
