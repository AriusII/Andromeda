use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash};

use crate::ProtocolLayout;
use crate::version::ProtocolVersion as SupportedProtocolVersion;

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

pub fn validate_catalog_procedure_manifest_resolution_request(
    request: &contract::v1::CatalogProcedureManifestResolutionRequest,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(request.protocol_major, request.protocol_minor)?;

    if request.request_id == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution request_id must be nonzero",
        ));
    }

    match &request.selector {
        Some(contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureId(
            procedure_id,
        )) if *procedure_id != 0 => {}
        Some(
            contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
                procedure_name,
            ),
        ) if !procedure_name.trim().is_empty() => {}
        Some(_) => {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest resolution selector must be nonzero/non-empty",
            ));
        }
        None => {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest resolution request requires a selector",
            ));
        }
    }

    validate_optional_contract_hash(
        "catalog manifest resolution expected_contract_hash",
        request.expected_contract_hash.as_deref(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution expected_catalog_version",
        request.expected_catalog_version,
    )?;

    Ok(())
}

pub fn validate_catalog_procedure_manifest_resolution_response(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(response.protocol_major, response.protocol_minor)?;

    if response.request_id == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution response request_id must be nonzero",
        ));
    }

    validate_resolution_status(response.status)?;
    validate_optional_contract_hash(
        "catalog manifest resolution resolved_contract_hash",
        response.resolved_contract_hash.as_deref(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution resolved_catalog_version",
        response.resolved_catalog_version,
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution current_catalog_version",
        response.current_catalog_version,
    )?;

    if response.status == 1 {
        let manifest = response.manifest.as_ref().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved catalog manifest response requires a manifest",
            )
        })?;
        validate_generated_procedure_manifest(manifest)?;

        if response.resolved_contract_hash.as_deref() != Some(manifest.contract_hash.as_slice()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved contract hash must match manifest contract_hash",
            ));
        }

        if response.resolved_catalog_version != Some(manifest.catalog_version) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved catalog version must match manifest catalog_version",
            ));
        }
    } else if response.manifest.is_some() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "non-resolved catalog manifest response must not carry a manifest",
        ));
    }

    Ok(())
}

fn validate_generated_protocol_version(major: u32, minor: u32) -> AndromedaResult<()> {
    SupportedProtocolVersion { major, minor }.validate()
}

fn validate_resolution_status(status: i32) -> AndromedaResult<()> {
    match status {
        1..=6 => Ok(()),
        0 => Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution status must be specified",
        )),
        _ => Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "unknown catalog manifest resolution status",
        )),
    }
}

fn validate_generated_procedure_manifest(
    manifest: &contract::v1::ProcedureManifest,
) -> AndromedaResult<()> {
    if manifest.procedure_id == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "resolved procedure manifest procedure_id must be nonzero",
        ));
    }

    if manifest.procedure_name.trim().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "resolved procedure manifest procedure_name must be non-empty",
        ));
    }

    validate_required_contract_hash(
        "resolved procedure manifest contract_hash",
        &manifest.contract_hash,
    )?;
    validate_optional_catalog_version(
        "resolved procedure manifest catalog_version",
        Some(manifest.catalog_version),
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest policy_version",
        &manifest.policy_version,
    )?;

    let protocol_layout = manifest.protocol_layout.as_ref().ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Contract,
            "resolved procedure manifest requires protocol_layout",
        )
    })?;
    validate_required_contract_hash(
        "resolved procedure manifest descriptor_set_hash",
        &protocol_layout.descriptor_set_hash,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest frame_envelope_hash",
        &protocol_layout.frame_envelope_hash,
    )?;
    if protocol_layout.protocol_package != PROTOCOL_PACKAGE {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "resolved procedure manifest protocol_package mismatch",
        ));
    }
    if protocol_layout.contract_package != CONTRACT_PACKAGE {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "resolved procedure manifest contract_package mismatch",
        ));
    }

    Ok(())
}

fn validate_optional_catalog_version(label: &str, version: Option<u64>) -> AndromedaResult<()> {
    if version == Some(0) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must be nonzero when present"),
        ));
    }

    Ok(())
}

fn validate_optional_contract_hash(label: &str, bytes: Option<&[u8]>) -> AndromedaResult<()> {
    if let Some(bytes) = bytes {
        validate_required_contract_hash(label, bytes)?;
    }

    Ok(())
}

fn validate_required_contract_hash(label: &str, bytes: &[u8]) -> AndromedaResult<()> {
    if bytes.len() != ContractHash::LEN {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must be 32 bytes"),
        ));
    }

    if bytes.iter().all(|byte| *byte == 0) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must not be zero"),
        ));
    }

    Ok(())
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
