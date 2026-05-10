#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Protobuf Schema Boundary

This crate owns the `.proto` sources, prost build output, generated module
wrappers, descriptor-set bytes, and schema-governance hashes for Andromeda
protocol and contract schemas.

Runtime-free protocol ownership is intentionally split:

| Surface | Owner |
| --- | --- |
| Generated prost modules and descriptor hashes | `andromeda-proto` |
| Generated message validation and protobuf projection | `andromeda-proto-wire` |
| Frame, stream, envelope, backpressure, and protocol invariants | `andromeda-rpc-protocol` |
| Typed frame/envelope codec helpers | `andromeda-rpc-codec` |
| Procedure contract and gateway manifest semantics | `andromeda-procedure-contract` |
| Structured object layout and row-count descriptors | `andromeda-structured-object` |

`andromeda-proto` is kept as the schema and governance surface: generated prost
access, descriptor bytes, and schema hashes remain anchored here. Runtime code
must import validation, projection, wire behavior, procedure contracts, and
structured object layouts from the owner crates above instead of treating this
crate as a general protocol surface.

Schema files must remain deterministic, service-free, and runtime-free. They do
not define gRPC services, HTTP transports, JSON mapping policy, or executor
behavior.
"#]

pub mod generated;
mod generated_validation;

use andromeda_procedure_contract::ProtocolLayout;

// Schema-owned stable surface: generated prost modules and schema-governance
// helpers stay anchored in `andromeda-proto`.
pub use generated::{
    descriptor_set_bytes, descriptor_set_hash, frame_envelope_hash, protocol_layout,
};
