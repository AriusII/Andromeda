use andromeda_wal::Lsn;
use sha2::{Digest, Sha256};

use crate::quorum_runtime::{FencingPolicy, ReplicationMode};

use super::WalShipmentRange;

const EVIDENCE_MAGIC: [u8; 8] = *b"ADRWSEV0";
const EVIDENCE_VERSION_V0: u16 = 0;
const FRAME_CHECKSUM_LEN: usize = 32;
const FRAME_HEADER_LEN: usize = EVIDENCE_MAGIC.len() + 2 + 4 + FRAME_CHECKSUM_LEN;
const EVIDENCE_DIGEST_LEN: usize = 32;
const PAYLOAD_FIELD_COUNT_U64: usize = 10;
const PAYLOAD_FIELD_COUNT_U8: usize = 3;
const PAYLOAD_LEN: usize = PAYLOAD_FIELD_COUNT_U64 * std::mem::size_of::<u64>()
    + PAYLOAD_FIELD_COUNT_U8
    + EVIDENCE_DIGEST_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalShippingEvidenceRejectionReason {
    ReplicaNotRequired,
    AckExceedsShippedMax,
    AckOutsideDurablePrefix,
    AckBeforeDurablePrefix,
    AckStale,
}

impl WalShippingEvidenceRejectionReason {
    const fn encode(self) -> u8 {
        match self {
            Self::ReplicaNotRequired => 1,
            Self::AckExceedsShippedMax => 2,
            Self::AckOutsideDurablePrefix => 3,
            Self::AckBeforeDurablePrefix => 4,
            Self::AckStale => 5,
        }
    }

    fn decode(tag: u8) -> Result<Option<Self>, WalShippingEvidenceCodecRejection> {
        match tag {
            0 => Ok(None),
            1 => Ok(Some(Self::ReplicaNotRequired)),
            2 => Ok(Some(Self::AckExceedsShippedMax)),
            3 => Ok(Some(Self::AckOutsideDurablePrefix)),
            4 => Ok(Some(Self::AckBeforeDurablePrefix)),
            5 => Ok(Some(Self::AckStale)),
            _ => Err(WalShippingEvidenceCodecRejection::InvalidRejectionReasonTag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalShippingEvidenceCodecRejection {
    Truncated,
    MagicMismatch,
    UnsupportedVersion,
    LengthOverflow,
    LengthMismatch,
    ChecksumMismatch,
    InvalidNodeIdentity,
    InvalidValidatedRange,
    InvalidReplicationModeTag,
    InvalidFencingPolicyTag,
    InvalidRejectionReasonTag,
    AckBeyondDurablePrefix,
    AppliedBeyondAck,
}

impl WalShippingEvidenceCodecRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Truncated => "wal shipping evidence frame is truncated",
            Self::MagicMismatch => "wal shipping evidence frame magic mismatch",
            Self::UnsupportedVersion => "wal shipping evidence frame version is unsupported",
            Self::LengthOverflow => "wal shipping evidence frame length overflow",
            Self::LengthMismatch => "wal shipping evidence frame length mismatch",
            Self::ChecksumMismatch => "wal shipping evidence frame checksum mismatch",
            Self::InvalidNodeIdentity => "wal shipping evidence node identity must not be zero",
            Self::InvalidValidatedRange => "wal shipping evidence validated range is invalid",
            Self::InvalidReplicationModeTag => {
                "wal shipping evidence replication mode tag is invalid"
            },
            Self::InvalidFencingPolicyTag => "wal shipping evidence fencing policy tag is invalid",
            Self::InvalidRejectionReasonTag => {
                "wal shipping evidence rejection reason tag is invalid"
            },
            Self::AckBeyondDurablePrefix => {
                "wal shipping evidence ack LSN must not exceed durable prefix"
            },
            Self::AppliedBeyondAck => "wal shipping evidence applied LSN must not exceed ack LSN",
        }
    }
}

impl std::fmt::Display for WalShippingEvidenceCodecRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for WalShippingEvidenceCodecRejection {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalShippingEvidenceV0 {
    pub source_node_id: u64,
    pub destination_node_id: u64,
    pub epoch: u64,
    pub validated_range: WalShipmentRange,
    pub durable_prefix_lsn: Lsn,
    pub ack_lsn: Lsn,
    pub applied_lsn: Lsn,
    pub replication_mode: ReplicationMode,
    pub fencing_policy: FencingPolicy,
    pub rpo_lag_bytes: u64,
    pub shipment_digest: [u8; EVIDENCE_DIGEST_LEN],
    pub rejection_reason: Option<WalShippingEvidenceRejectionReason>,
}

impl WalShippingEvidenceV0 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_node_id: u64,
        destination_node_id: u64,
        epoch: u64,
        validated_range: WalShipmentRange,
        durable_prefix_lsn: Lsn,
        ack_lsn: Lsn,
        applied_lsn: Lsn,
        replication_mode: ReplicationMode,
        fencing_policy: FencingPolicy,
        rpo_lag_bytes: u64,
        shipment_digest: [u8; EVIDENCE_DIGEST_LEN],
        rejection_reason: Option<WalShippingEvidenceRejectionReason>,
    ) -> Result<Self, WalShippingEvidenceCodecRejection> {
        validate_ids(source_node_id, destination_node_id)?;
        validate_range(validated_range)?;
        if ack_lsn > durable_prefix_lsn {
            return Err(WalShippingEvidenceCodecRejection::AckBeyondDurablePrefix);
        }
        if applied_lsn > ack_lsn {
            return Err(WalShippingEvidenceCodecRejection::AppliedBeyondAck);
        }

        Ok(Self {
            source_node_id,
            destination_node_id,
            epoch,
            validated_range,
            durable_prefix_lsn,
            ack_lsn,
            applied_lsn,
            replication_mode,
            fencing_policy,
            rpo_lag_bytes,
            shipment_digest,
            rejection_reason,
        })
    }

    pub fn encode_le(&self) -> Result<Vec<u8>, WalShippingEvidenceCodecRejection> {
        let mut payload = Vec::with_capacity(PAYLOAD_LEN);
        push_u64(&mut payload, self.source_node_id);
        push_u64(&mut payload, self.destination_node_id);
        push_u64(&mut payload, self.epoch);
        push_u64(&mut payload, self.validated_range.first.get());
        push_u64(&mut payload, self.validated_range.last.get());
        push_u64(
            &mut payload,
            u64::try_from(self.validated_range.count)
                .map_err(|_| WalShippingEvidenceCodecRejection::LengthOverflow)?,
        );
        push_u64(&mut payload, self.durable_prefix_lsn.get());
        push_u64(&mut payload, self.ack_lsn.get());
        push_u64(&mut payload, self.applied_lsn.get());
        payload.push(encode_replication_mode(self.replication_mode));
        payload.push(encode_fencing_policy(self.fencing_policy));
        payload.push(self.rejection_reason.map_or(0, |reason| reason.encode()));
        push_u64(&mut payload, self.rpo_lag_bytes);
        payload.extend_from_slice(&self.shipment_digest);

        let payload_len = u32::try_from(payload.len())
            .map_err(|_| WalShippingEvidenceCodecRejection::LengthOverflow)?;
        let checksum = Sha256::digest(&payload);
        let frame_len = FRAME_HEADER_LEN
            .checked_add(payload.len())
            .ok_or(WalShippingEvidenceCodecRejection::LengthOverflow)?;
        let mut frame = Vec::with_capacity(frame_len);
        frame.extend_from_slice(&EVIDENCE_MAGIC);
        frame.extend_from_slice(&EVIDENCE_VERSION_V0.to_le_bytes());
        frame.extend_from_slice(&payload_len.to_le_bytes());
        frame.extend_from_slice(&checksum);
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    pub fn decode_le(encoded: &[u8]) -> Result<Self, WalShippingEvidenceCodecRejection> {
        if encoded.len() < FRAME_HEADER_LEN {
            return Err(WalShippingEvidenceCodecRejection::Truncated);
        }
        if encoded[..EVIDENCE_MAGIC.len()] != EVIDENCE_MAGIC {
            return Err(WalShippingEvidenceCodecRejection::MagicMismatch);
        }
        let version = read_u16(&encoded[EVIDENCE_MAGIC.len()..EVIDENCE_MAGIC.len() + 2])?;
        if version != EVIDENCE_VERSION_V0 {
            return Err(WalShippingEvidenceCodecRejection::UnsupportedVersion);
        }
        let payload_len = usize::try_from(read_u32(
            &encoded[EVIDENCE_MAGIC.len() + 2..EVIDENCE_MAGIC.len() + 6],
        )?)
        .map_err(|_| WalShippingEvidenceCodecRejection::LengthOverflow)?;
        let expected_len = FRAME_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(WalShippingEvidenceCodecRejection::LengthOverflow)?;
        if encoded.len() != expected_len {
            return Err(WalShippingEvidenceCodecRejection::LengthMismatch);
        }
        let payload = &encoded[FRAME_HEADER_LEN..];
        let checksum = Sha256::digest(payload);
        let checksum_start = EVIDENCE_MAGIC.len() + 2 + 4;
        let checksum_end = checksum_start + FRAME_CHECKSUM_LEN;
        if checksum[..] != encoded[checksum_start..checksum_end] {
            return Err(WalShippingEvidenceCodecRejection::ChecksumMismatch);
        }
        if payload.len() != PAYLOAD_LEN {
            return Err(WalShippingEvidenceCodecRejection::LengthMismatch);
        }

        let mut cursor = 0usize;
        let source_node_id = read_u64_from_payload(payload, &mut cursor)?;
        let destination_node_id = read_u64_from_payload(payload, &mut cursor)?;
        validate_ids(source_node_id, destination_node_id)?;
        let epoch = read_u64_from_payload(payload, &mut cursor)?;
        let validated_first = Lsn::new(read_u64_from_payload(payload, &mut cursor)?);
        let validated_last = Lsn::new(read_u64_from_payload(payload, &mut cursor)?);
        let validated_count = usize::try_from(read_u64_from_payload(payload, &mut cursor)?)
            .map_err(|_| WalShippingEvidenceCodecRejection::LengthOverflow)?;
        let validated_range = WalShipmentRange {
            first: validated_first,
            last: validated_last,
            count: validated_count,
        };
        validate_range(validated_range)?;
        let durable_prefix_lsn = Lsn::new(read_u64_from_payload(payload, &mut cursor)?);
        let ack_lsn = Lsn::new(read_u64_from_payload(payload, &mut cursor)?);
        let applied_lsn = Lsn::new(read_u64_from_payload(payload, &mut cursor)?);
        let replication_mode =
            decode_replication_mode(read_u8_from_payload(payload, &mut cursor)?)?;
        let fencing_policy = decode_fencing_policy(read_u8_from_payload(payload, &mut cursor)?)?;
        let rejection_reason = WalShippingEvidenceRejectionReason::decode(read_u8_from_payload(
            payload,
            &mut cursor,
        )?)?;
        let rpo_lag_bytes = read_u64_from_payload(payload, &mut cursor)?;
        let mut shipment_digest = [0u8; EVIDENCE_DIGEST_LEN];
        let digest_end = cursor
            .checked_add(EVIDENCE_DIGEST_LEN)
            .ok_or(WalShippingEvidenceCodecRejection::LengthOverflow)?;
        if digest_end > payload.len() {
            return Err(WalShippingEvidenceCodecRejection::Truncated);
        }
        shipment_digest.copy_from_slice(&payload[cursor..digest_end]);
        cursor = digest_end;
        if cursor != payload.len() {
            return Err(WalShippingEvidenceCodecRejection::LengthMismatch);
        }

        Self::new(
            source_node_id,
            destination_node_id,
            epoch,
            validated_range,
            durable_prefix_lsn,
            ack_lsn,
            applied_lsn,
            replication_mode,
            fencing_policy,
            rpo_lag_bytes,
            shipment_digest,
            rejection_reason,
        )
    }
}

fn validate_ids(
    source_node_id: u64,
    destination_node_id: u64,
) -> Result<(), WalShippingEvidenceCodecRejection> {
    if source_node_id == 0 || destination_node_id == 0 {
        return Err(WalShippingEvidenceCodecRejection::InvalidNodeIdentity);
    }
    Ok(())
}

fn validate_range(range: WalShipmentRange) -> Result<(), WalShippingEvidenceCodecRejection> {
    if range.count == 0 || range.first > range.last {
        return Err(WalShippingEvidenceCodecRejection::InvalidValidatedRange);
    }
    let span = range
        .last
        .get()
        .checked_sub(range.first.get())
        .and_then(|delta| delta.checked_add(1))
        .ok_or(WalShippingEvidenceCodecRejection::InvalidValidatedRange)?;
    let count = u64::try_from(range.count)
        .map_err(|_| WalShippingEvidenceCodecRejection::LengthOverflow)?;
    if span != count {
        return Err(WalShippingEvidenceCodecRejection::InvalidValidatedRange);
    }
    Ok(())
}

fn push_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn read_u8_from_payload(
    payload: &[u8],
    cursor: &mut usize,
) -> Result<u8, WalShippingEvidenceCodecRejection> {
    let end = cursor
        .checked_add(1)
        .ok_or(WalShippingEvidenceCodecRejection::LengthOverflow)?;
    if end > payload.len() {
        return Err(WalShippingEvidenceCodecRejection::Truncated);
    }
    let value = payload[*cursor];
    *cursor = end;
    Ok(value)
}

fn read_u64_from_payload(
    payload: &[u8],
    cursor: &mut usize,
) -> Result<u64, WalShippingEvidenceCodecRejection> {
    let end = cursor
        .checked_add(8)
        .ok_or(WalShippingEvidenceCodecRejection::LengthOverflow)?;
    if end > payload.len() {
        return Err(WalShippingEvidenceCodecRejection::Truncated);
    }
    let value = read_u64(&payload[*cursor..end])?;
    *cursor = end;
    Ok(value)
}

fn read_u16(bytes: &[u8]) -> Result<u16, WalShippingEvidenceCodecRejection> {
    let array: [u8; 2] = bytes
        .try_into()
        .map_err(|_| WalShippingEvidenceCodecRejection::Truncated)?;
    Ok(u16::from_le_bytes(array))
}

fn read_u32(bytes: &[u8]) -> Result<u32, WalShippingEvidenceCodecRejection> {
    let array: [u8; 4] = bytes
        .try_into()
        .map_err(|_| WalShippingEvidenceCodecRejection::Truncated)?;
    Ok(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8]) -> Result<u64, WalShippingEvidenceCodecRejection> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| WalShippingEvidenceCodecRejection::Truncated)?;
    Ok(u64::from_le_bytes(array))
}

fn encode_replication_mode(mode: ReplicationMode) -> u8 {
    match mode {
        ReplicationMode::Asynchronous => 0,
        ReplicationMode::QuorumEnforced => 1,
    }
}

fn decode_replication_mode(tag: u8) -> Result<ReplicationMode, WalShippingEvidenceCodecRejection> {
    match tag {
        0 => Ok(ReplicationMode::Asynchronous),
        1 => Ok(ReplicationMode::QuorumEnforced),
        _ => Err(WalShippingEvidenceCodecRejection::InvalidReplicationModeTag),
    }
}

fn encode_fencing_policy(policy: FencingPolicy) -> u8 {
    match policy {
        FencingPolicy::Allow => 0,
        FencingPolicy::BlockOnQuorumLoss => 1,
    }
}

fn decode_fencing_policy(tag: u8) -> Result<FencingPolicy, WalShippingEvidenceCodecRejection> {
    match tag {
        0 => Ok(FencingPolicy::Allow),
        1 => Ok(FencingPolicy::BlockOnQuorumLoss),
        _ => Err(WalShippingEvidenceCodecRejection::InvalidFencingPolicyTag),
    }
}
