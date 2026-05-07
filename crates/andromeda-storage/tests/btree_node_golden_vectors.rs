//! B-tree node golden-vector and format gate suite.
//!
//! The suite keeps stable byte-format vectors separate from constructor,
//! roundtrip, and decode rejection gates.

#[path = "btree_node_golden_vectors/decode_body_gates.rs"]
mod decode_body_gates;
#[path = "btree_node_golden_vectors/decode_header_gates.rs"]
mod decode_header_gates;
#[path = "btree_node_golden_vectors/golden_full_nodes.rs"]
mod golden_full_nodes;
#[path = "btree_node_golden_vectors/golden_header.rs"]
mod golden_header;
#[path = "btree_node_golden_vectors/persistence_validation.rs"]
mod persistence_validation;
#[path = "btree_node_golden_vectors/roundtrip.rs"]
mod roundtrip;
#[path = "btree_node_golden_vectors/support.rs"]
mod support;
