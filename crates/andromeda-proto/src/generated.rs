use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash};

use crate::ProtocolLayout;

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
    ContractHash::new(stable_hash_256(
        b"andromeda-descriptor-set",
        DESCRIPTOR_SET_BYTES,
    ))
}

pub fn frame_envelope_hash() -> ContractHash {
    ContractHash::new(stable_hash_256(
        PROTOCOL_FRAME_ENVELOPE_TYPE.as_bytes(),
        DESCRIPTOR_SET_BYTES,
    ))
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
    message.encode_to_vec()
}

pub fn decode_generated_message<M>(bytes: &[u8]) -> AndromedaResult<M>
where
    M: prost::Message + Default,
{
    if bytes.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "generated protobuf decode failed: empty bytes are not valid protobuf",
        ));
    }

    M::decode(bytes).map_err(|error| {
        AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!("generated protobuf decode failed: {error}"),
        )
    })
}

fn stable_hash_256(domain: &[u8], bytes: &[u8]) -> [u8; ContractHash::LEN] {
    let mut output = [0_u8; ContractHash::LEN];
    let seeds = [
        0xcbf2_9ce4_8422_2325_u64,
        0x8422_2325_cbf2_9ce4_u64,
        0x9e37_79b9_7f4a_7c15_u64,
        0x94d0_49bb_1331_11eb_u64,
    ];

    for (index, seed) in seeds.into_iter().enumerate() {
        let hash = stable_hash64(seed, domain, bytes);
        let start = index * 8;
        output[start..start + 8].copy_from_slice(&hash.to_be_bytes());
    }

    output
}

fn stable_hash64(seed: u64, domain: &[u8], bytes: &[u8]) -> u64 {
    let mut hash = seed ^ ((domain.len() as u64) << 32) ^ bytes.len() as u64;

    for byte in domain {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(17);
    }

    hash ^= 0xff;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(31);
    }

    hash
}
