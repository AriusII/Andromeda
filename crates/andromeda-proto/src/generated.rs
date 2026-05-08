use andromeda_error::AndromedaResult;
use andromeda_types::ContractHash;

use crate::ProtocolLayout;
pub use crate::generated_validation::{
    project_generated_frame_envelope, validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response, validate_generated_error_envelope,
    validate_generated_frame_envelope, validate_generated_invocation_request,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_execute_request, validate_generated_rpc_metadata,
};

pub const PROTOCOL_PACKAGE: &str = "andromeda.protocol.v1";
pub const CONTRACT_PACKAGE: &str = "andromeda.contract.v1";
pub const PROTOCOL_FRAME_ENVELOPE_TYPE: &str = "andromeda.protocol.v1.FrameEnvelope";
pub const DESCRIPTOR_SET_HASH_ALGORITHM: &str = "andromeda-stable-fnv1a-256-v1";

const DESCRIPTOR_SET_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/andromeda_descriptor.bin"));

pub mod andromeda {
    pub mod contract {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/andromeda.contract.v1.rs"));
        }
    }

    pub mod protocol {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/andromeda.protocol.v1.rs"));
        }
    }
}

pub use andromeda::{contract, protocol};

pub fn descriptor_set_bytes() -> &'static [u8] {
    DESCRIPTOR_SET_BYTES
}

pub fn descriptor_set_hash() -> ContractHash {
    andromeda_proto_wire::stable_contract_hash(b"andromeda-descriptor-set", DESCRIPTOR_SET_BYTES)
}

pub fn frame_envelope_hash() -> ContractHash {
    andromeda_proto_wire::stable_contract_hash(
        PROTOCOL_FRAME_ENVELOPE_TYPE.as_bytes(),
        DESCRIPTOR_SET_BYTES,
    )
}

pub fn protocol_layout() -> ProtocolLayout {
    ProtocolLayout {
        descriptor_set_hash: descriptor_set_hash(),
        frame_envelope_hash: frame_envelope_hash(),
    }
}

pub fn encode_generated_message<M>(message: &M) -> Vec<u8>
where
    M: prost::Message,
{
    andromeda_proto_wire::encode_protobuf_message(message)
}

pub fn decode_generated_message<M>(bytes: &[u8]) -> AndromedaResult<M>
where
    M: prost::Message + Default,
{
    andromeda_proto_wire::decode_protobuf_message(bytes, "generated protobuf")
}
