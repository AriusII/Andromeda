use crate::TraceId;

use super::{CriticalDecisionKind, CriticalDecisionTrace, ProtocolEventScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolRejectionReason {
    /// Header CRC32 did not match the recomputed value over the zeroed-CRC
    /// header bytes.
    HeaderCrcMismatch,
    /// Frame codec or protocol version offered by the peer is not supported
    /// by this build.
    UnsupportedVersion,
    /// Frame was dispatched on a surface plane that does not match the
    /// connection's negotiated plane.
    SurfacePlaneMismatch,
    /// Frame family is not permitted on the connection's surface plane.
    FrameFamilyNotPermitted,
    /// Result-stream sequence (metadata → batch* → completion) was violated.
    ResultStreamSequenceViolation,
    /// Frame payload length exceeds `MAX_FRAME_PAYLOAD_LENGTH` or contradicts
    /// declared payload length.
    OversizedPayload,
    /// Command frame received before the session reached `Active` (i.e. before
    /// `Hello`/`Auth` completed) or after `Closed`/`Draining` for command
    /// frames.
    PreAuthCommandRejected,
    /// Catch-all for framing/protocol errors that do not fit the buckets
    /// above. Constructors must supply a precise textual reason.
    OtherFraming,
}

impl ProtocolRejectionReason {
    pub const fn is_pre_auth_capable(self) -> bool {
        matches!(
            self,
            Self::HeaderCrcMismatch
                | Self::UnsupportedVersion
                | Self::SurfacePlaneMismatch
                | Self::FrameFamilyNotPermitted
                | Self::OversizedPayload
                | Self::PreAuthCommandRejected
                | Self::OtherFraming
        )
    }

    pub const fn is_safety_critical(self) -> bool {
        matches!(
            self,
            Self::HeaderCrcMismatch
                | Self::OversizedPayload
                | Self::PreAuthCommandRejected
                | Self::SurfacePlaneMismatch
        )
    }
}

/// Surface plane of the connection where the rejection happened.
///
/// Mirrors the QUIC `SurfacePlane` enum without taking a runtime dependency
/// on `andromeda-quic` from `andromeda-observe`. Constructors in the QUIC
/// crate translate from the live enum into this typed code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolSurfacePlane {
    Application,
    Administration,
    HighAvailability,
    Monitoring,
}

impl ProtocolSurfacePlane {
    pub const fn wire_code(self) -> u16 {
        match self {
            Self::Application => 1,
            Self::Administration => 2,
            Self::HighAvailability => 3,
            Self::Monitoring => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolRejectionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub reason: ProtocolRejectionReason,
    pub connection_id: Option<u64>,
    pub session_id: Option<u64>,
    pub request_id: Option<u64>,
    pub frame_type_code: Option<u32>,
    pub surface_plane: Option<ProtocolSurfacePlane>,
    pub protocol_version: Option<u16>,
    pub payload_length: Option<u64>,
    pub sequence_position: Option<u64>,
    pub pre_auth: bool,
    pub detail: String,
}

impl ProtocolRejectionTrace {
    /// Returns true once a non-empty textual detail has been supplied.
    pub fn has_detail(&self) -> bool {
        !self.detail.trim().is_empty()
    }

    /// Returns true when the trace carries enough evidence for forensic
    /// retention: trace id + reason + at least one correlator beyond the
    /// reason enum itself.
    pub fn has_minimum_evidence(&self) -> bool {
        if self.trace_id.is_zero() || !self.has_detail() {
            return false;
        }
        match self.reason {
            ProtocolRejectionReason::HeaderCrcMismatch => true,
            ProtocolRejectionReason::UnsupportedVersion => self.protocol_version.is_some(),
            ProtocolRejectionReason::SurfacePlaneMismatch => self.surface_plane.is_some(),
            ProtocolRejectionReason::FrameFamilyNotPermitted => {
                self.surface_plane.is_some() && self.frame_type_code.is_some()
            },
            ProtocolRejectionReason::ResultStreamSequenceViolation => {
                self.frame_type_code.is_some()
            },
            ProtocolRejectionReason::OversizedPayload => self.payload_length.is_some(),
            ProtocolRejectionReason::PreAuthCommandRejected => self.pre_auth,
            ProtocolRejectionReason::OtherFraming => true,
        }
    }

    /// Lossy projection into the existing `DecisionTrace` envelope so that
    /// callers without a `ProtocolRejectionTrace`-aware sink can still ship
    /// evidence. The mapping uses `CriticalDecisionKind::FrameRejection` for
    /// framing rejections and `CriticalDecisionKind::UnsupportedVersion` for
    /// version mismatch.
    pub fn as_decision_trace(&self) -> CriticalDecisionTrace {
        let decision = match self.reason {
            ProtocolRejectionReason::UnsupportedVersion => CriticalDecisionKind::UnsupportedVersion,
            ProtocolRejectionReason::FrameFamilyNotPermitted
            | ProtocolRejectionReason::SurfacePlaneMismatch => {
                CriticalDecisionKind::StreamRoleRejection
            },
            _ => CriticalDecisionKind::FrameRejection,
        };
        CriticalDecisionTrace {
            trace_id: self.trace_id,
            decision,
            reason: self.detail.clone(),
        }
    }

    /// Constructor: header CRC mismatch detected during decode.
    pub fn header_crc_mismatch(
        trace_id: TraceId,
        connection_id: Option<u64>,
        surface_plane: Option<ProtocolSurfacePlane>,
        pre_auth: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Connection,
            reason: ProtocolRejectionReason::HeaderCrcMismatch,
            connection_id,
            session_id: None,
            request_id: None,
            frame_type_code: None,
            surface_plane,
            protocol_version: None,
            payload_length: None,
            sequence_position: None,
            pre_auth,
            detail: detail.into(),
        }
    }

    /// Constructor: unsupported codec/protocol version observed in `Hello`
    /// or header decode.
    pub fn unsupported_version(
        trace_id: TraceId,
        connection_id: Option<u64>,
        observed_version: u16,
        pre_auth: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Connection,
            reason: ProtocolRejectionReason::UnsupportedVersion,
            connection_id,
            session_id: None,
            request_id: None,
            frame_type_code: None,
            surface_plane: None,
            protocol_version: Some(observed_version),
            payload_length: None,
            sequence_position: None,
            pre_auth,
            detail: detail.into(),
        }
    }

    /// Constructor: frame dispatched on the wrong surface plane.
    pub fn surface_plane_mismatch(
        trace_id: TraceId,
        session_id: Option<u64>,
        request_id: Option<u64>,
        bound_plane: ProtocolSurfacePlane,
        requested_plane: ProtocolSurfacePlane,
        frame_type_code: Option<u32>,
        pre_auth: bool,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Session,
            reason: ProtocolRejectionReason::SurfacePlaneMismatch,
            connection_id: None,
            session_id,
            request_id,
            frame_type_code,
            surface_plane: Some(bound_plane),
            protocol_version: None,
            payload_length: None,
            sequence_position: None,
            pre_auth,
            detail: format!(
                "frame requested plane code {} on connection bound to plane code {}",
                requested_plane.wire_code(),
                bound_plane.wire_code()
            ),
        }
    }

    /// Constructor: frame family not permitted on the bound surface plane.
    pub fn frame_family_blocked(
        trace_id: TraceId,
        session_id: Option<u64>,
        request_id: Option<u64>,
        bound_plane: ProtocolSurfacePlane,
        frame_type_code: u32,
        pre_auth: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Session,
            reason: ProtocolRejectionReason::FrameFamilyNotPermitted,
            connection_id: None,
            session_id,
            request_id,
            frame_type_code: Some(frame_type_code),
            surface_plane: Some(bound_plane),
            protocol_version: None,
            payload_length: None,
            sequence_position: None,
            pre_auth,
            detail: detail.into(),
        }
    }

    /// Constructor: result stream sequence (metadata → batch → completion)
    /// violated.
    pub fn result_stream_sequence_violation(
        trace_id: TraceId,
        session_id: Option<u64>,
        request_id: Option<u64>,
        frame_type_code: u32,
        sequence_position: Option<u64>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Request,
            reason: ProtocolRejectionReason::ResultStreamSequenceViolation,
            connection_id: None,
            session_id,
            request_id,
            frame_type_code: Some(frame_type_code),
            surface_plane: None,
            protocol_version: None,
            payload_length: None,
            sequence_position,
            pre_auth: false,
            detail: detail.into(),
        }
    }

    /// Constructor: declared or actual payload length exceeded
    /// `MAX_FRAME_PAYLOAD_LENGTH` or violated declared length.
    pub fn oversized_payload(
        trace_id: TraceId,
        session_id: Option<u64>,
        request_id: Option<u64>,
        frame_type_code: Option<u32>,
        payload_length: u64,
        pre_auth: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: if pre_auth {
                ProtocolEventScope::Connection
            } else {
                ProtocolEventScope::Request
            },
            reason: ProtocolRejectionReason::OversizedPayload,
            connection_id: None,
            session_id,
            request_id,
            frame_type_code,
            surface_plane: None,
            protocol_version: None,
            payload_length: Some(payload_length),
            sequence_position: None,
            pre_auth,
            detail: detail.into(),
        }
    }

    /// Constructor: command frame received before authentication completed.
    pub fn pre_auth_command(
        trace_id: TraceId,
        connection_id: Option<u64>,
        session_id: Option<u64>,
        frame_type_code: u32,
        bound_plane: Option<ProtocolSurfacePlane>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            trace_id,
            scope: ProtocolEventScope::Connection,
            reason: ProtocolRejectionReason::PreAuthCommandRejected,
            connection_id,
            session_id,
            request_id: None,
            frame_type_code: Some(frame_type_code),
            surface_plane: bound_plane,
            protocol_version: None,
            payload_length: None,
            sequence_position: None,
            pre_auth: true,
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_version_requires_version_evidence() {
        let trace = ProtocolRejectionTrace::unsupported_version(
            TraceId::new(2),
            Some(11),
            42,
            true,
            "unsupported QUIC frame codec version",
        );

        assert_eq!(trace.protocol_version, Some(42));
        assert_eq!(trace.reason, ProtocolRejectionReason::UnsupportedVersion);
        assert!(trace.has_minimum_evidence());
        assert_eq!(
            trace.as_decision_trace().decision,
            CriticalDecisionKind::UnsupportedVersion
        );
    }

    #[test]
    fn rejection_minimum_evidence_rejects_zero_trace_or_empty_detail() {
        let zero_trace = ProtocolRejectionTrace::header_crc_mismatch(
            TraceId::new(0),
            Some(1),
            None,
            true,
            "crc mismatch",
        );
        assert!(!zero_trace.has_minimum_evidence());

        let empty_detail = ProtocolRejectionTrace::header_crc_mismatch(
            TraceId::new(1),
            Some(1),
            None,
            true,
            "   ",
        );
        assert!(!empty_detail.has_minimum_evidence());
    }
}
