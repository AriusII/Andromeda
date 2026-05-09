use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

use crate::{
    FrameEnvelope, PayloadKind, ProtocolVersion, validate_optional_contract_hash,
    validate_required_contract_hash,
};

use super::views::GeneratedProtocolVersionView;
use super::{common::protocol_error, views::GeneratedFrameEnvelopeView};

pub fn validate_generated_protocol_version(major: u32, minor: u32) -> AndromedaResult<()> {
    ProtocolVersion { major, minor }.validate()
}

pub fn project_generated_frame_envelope<T>(envelope: &T) -> AndromedaResult<FrameEnvelope>
where
    T: GeneratedFrameEnvelopeView,
{
    let Some(version) = envelope.protocol_version() else {
        return protocol_error("generated frame envelope requires protocol_version");
    };

    let payload_kind = project_payload_kind(envelope.payload_kind())?;
    let contract_hash = project_contract_hash_for_payload_kind(
        "generated frame envelope contract_hash",
        payload_kind,
        envelope.contract_hash(),
    )?;

    FrameEnvelope {
        protocol_version: ProtocolVersion {
            major: version.major(),
            minor: version.minor(),
        },
        contract_hash,
        catalog_version: CatalogVersion::new(envelope.catalog_version()),
        request_id: RequestId::new(envelope.request_id()),
        session_id: SessionId::new(envelope.session_id()),
        tx_id: envelope.tx_id().map(TransactionId::new),
        payload_kind,
        payload: envelope.payload().to_vec(),
    }
    .validated()
}

pub fn validate_generated_frame_envelope<T>(envelope: &T) -> AndromedaResult<()>
where
    T: GeneratedFrameEnvelopeView,
{
    project_generated_frame_envelope(envelope).map(|_| ())
}

fn project_payload_kind(payload_kind: i32) -> AndromedaResult<PayloadKind> {
    let Ok(code) = u32::try_from(payload_kind) else {
        return protocol_error("unknown generated frame envelope payload_kind");
    };

    PayloadKind::try_from(code)
}

fn project_contract_hash_for_payload_kind(
    label: &str,
    payload_kind: PayloadKind,
    bytes: &[u8],
) -> AndromedaResult<ContractHash> {
    if bytes.is_empty() && !payload_kind.requires_contract_hash() {
        return Ok(ContractHash::zero());
    }

    if payload_kind.requires_contract_hash() {
        validate_required_contract_hash(label, bytes)?;
    } else {
        validate_optional_contract_hash(label, Some(bytes))?;
    }

    ContractHash::from_slice(bytes)
}
