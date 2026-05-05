//! RPC completion and transaction outcome types.
//!
//! This module defines the completion envelope which concludes an RPC stream,
//! including transaction outcome, row counts, and durability evidence.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

use crate::ProtocolVersion;

/// Stable wire-aligned terminal completion code for the in-memory
/// `RpcCompletionStatus` enum. Values must match the generated
/// `protocol::v1::rpc_completion::Status` integer codes 1..=8 declared in
/// `crates/andromeda-proto/proto/andromeda/protocol/v1/completion.proto`. Code
/// `0` is reserved for `STATUS_UNSPECIFIED` and is intentionally unreachable
/// from this enum.
///
/// Consumers MUST use this code instead of relying on `Debug`/`Display`
/// projections when emitting telemetry, journaling, or comparing statuses
/// across protocol versions.
pub type CompletionTerminalCode = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcCompletionStatus {
    Committed,
    RolledBack,
    FailedBeforeTransaction,
    Cancelled,
    Poisoned,
    PermissionDenied,
    ContractRejected,
    SystemUnavailable,
}

impl RpcCompletionStatus {
    /// Stable terminal completion code aligned with the generated
    /// `protocol::v1::rpc_completion::Status` integer values.
    pub const fn terminal_code(self) -> CompletionTerminalCode {
        match self {
            Self::Committed => 1,
            Self::RolledBack => 2,
            Self::FailedBeforeTransaction => 3,
            Self::Cancelled => 4,
            Self::Poisoned => 5,
            Self::PermissionDenied => 6,
            Self::ContractRejected => 7,
            Self::SystemUnavailable => 8,
        }
    }

    /// Reverse lookup from a stable wire code to the in-memory status.
    pub const fn from_terminal_code(code: CompletionTerminalCode) -> Option<Self> {
        match code {
            1 => Some(Self::Committed),
            2 => Some(Self::RolledBack),
            3 => Some(Self::FailedBeforeTransaction),
            4 => Some(Self::Cancelled),
            5 => Some(Self::Poisoned),
            6 => Some(Self::PermissionDenied),
            7 => Some(Self::ContractRejected),
            8 => Some(Self::SystemUnavailable),
            _ => None,
        }
    }

    /// Returns true when the status reports a transaction that reached a
    /// durable terminal state (committed or rolled back).
    pub const fn is_transactional_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOutcome {
    NotStarted,
    Committed,
    RolledBack,
    Failed,
    Cancelled,
}

impl TransactionOutcome {
    /// Stable wire-aligned terminal code matching
    /// `protocol::v1::rpc_completion::TransactionOutcome` values.
    pub const fn terminal_code(self) -> CompletionTerminalCode {
        match self {
            Self::NotStarted => 1,
            Self::Committed => 2,
            Self::RolledBack => 3,
            Self::Failed => 4,
            Self::Cancelled => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRowCountSummary {
    pub result_name: String,
    pub rows_emitted: u64,
    pub row_count_exact: Option<u64>,
}

impl ResultRowCountSummary {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.result_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result row-count summary name must not be empty",
            ));
        }

        if let Some(exact) = self.row_count_exact {
            if exact != self.rows_emitted {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "exact result row count must match emitted rows",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCompletion {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<String>,
    pub status: RpcCompletionStatus,
    pub transaction_outcome: TransactionOutcome,
    pub rows_affected: Option<u64>,
    pub result_row_counts: Vec<ResultRowCountSummary>,
    pub tx_id: Option<TransactionId>,
    pub durable_lsn: Option<u64>,
}

impl RpcCompletion {
    pub fn validate(&self) -> AndromedaResult<()> {
        if matches!(self.trace_id.as_deref(), Some(trace_id) if trace_id.trim().is_empty()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "trace correlation id must not be empty when present",
            ));
        }

        for summary in &self.result_row_counts {
            summary.validate()?;
        }

        match self.status {
            RpcCompletionStatus::Committed => {
                self.validate_transactional_completion(
                    TransactionOutcome::Committed,
                    "committed RPC completion",
                )?;
            }
            RpcCompletionStatus::RolledBack => {
                self.validate_transactional_completion(
                    TransactionOutcome::RolledBack,
                    "rolled-back RPC completion",
                )?;

                if self.rows_affected != Some(0) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "rolled-back RPC completion must report zero rows affected",
                    ));
                }
            }
            RpcCompletionStatus::FailedBeforeTransaction
            | RpcCompletionStatus::PermissionDenied
            | RpcCompletionStatus::ContractRejected => {
                self.validate_not_started_completion("pre-transaction RPC completion")?;
            }
            RpcCompletionStatus::Cancelled => {
                if self.transaction_outcome == TransactionOutcome::Committed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "cancelled RPC completion must not declare committed transaction outcome",
                    ));
                }
                self.validate_optional_transactional_binding("cancelled RPC completion")?;
            }
            RpcCompletionStatus::Poisoned | RpcCompletionStatus::SystemUnavailable => {
                if self.transaction_outcome == TransactionOutcome::Committed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "failed RPC completion must not declare committed transaction outcome",
                    ));
                }
                self.validate_optional_transactional_binding("failed RPC completion")?;
            }
        }

        if self.transaction_outcome == TransactionOutcome::Committed
            && self.status != RpcCompletionStatus::Committed
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "committed transaction outcome requires committed RPC status",
            ));
        }

        Ok(())
    }

    fn validate_transactional_completion(
        &self,
        expected_outcome: TransactionOutcome,
        label: &str,
    ) -> AndromedaResult<()> {
        if self.transaction_outcome != expected_outcome {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must declare matching transaction outcome"),
            ));
        }

        if self.tx_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must include transaction id"),
            ));
        }

        match self.durable_lsn {
            Some(lsn) if lsn > 0 => Ok(()),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("{label} must include nonzero durable LSN evidence"),
            )),
        }
    }

    fn validate_not_started_completion(&self, label: &str) -> AndromedaResult<()> {
        if self.transaction_outcome != TransactionOutcome::NotStarted {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must declare transaction not started"),
            ));
        }

        if self.tx_id.is_some() || self.durable_lsn.is_some() || self.rows_affected.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must not carry transaction result evidence"),
            ));
        }

        Ok(())
    }

    /// Tighten Poisoned/Cancelled/SystemUnavailable: when the transaction
    /// outcome reports a terminal transaction state, require matching
    /// transaction id and nonzero durable LSN evidence (so a failure cannot
    /// claim transactional terminal status without durable proof). When the
    /// outcome is `NotStarted`, no transaction evidence may be carried.
    fn validate_optional_transactional_binding(&self, label: &str) -> AndromedaResult<()> {
        match self.transaction_outcome {
            TransactionOutcome::NotStarted => {
                if self.tx_id.is_some() || self.durable_lsn.is_some() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "{label} declaring transaction not started must not carry transaction evidence"
                        ),
                    ));
                }
                Ok(())
            }
            TransactionOutcome::RolledBack => {
                if self.tx_id.is_none() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "{label} declaring rolled-back outcome must include transaction id"
                        ),
                    ));
                }
                match self.durable_lsn {
                    Some(lsn) if lsn > 0 => Ok(()),
                    _ => Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "{label} declaring rolled-back outcome must include nonzero durable LSN evidence"
                        ),
                    )),
                }
            }
            TransactionOutcome::Cancelled | TransactionOutcome::Failed => {
                // Non-durable transient outcomes may not carry durable LSN
                // claims. Tx id remains optional for diagnostic correlation.
                if self.durable_lsn.is_some() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "{label} declaring non-durable transaction outcome must not carry durable LSN evidence"
                        ),
                    ));
                }
                Ok(())
            }
            TransactionOutcome::Committed => Ok(()),
        }
    }

    /// Validate the completion against the negotiated protocol version. Use
    /// this at the protocol boundary to detect version drift between the
    /// completion-emitting executor and the wire envelope it travels in.
    pub fn validate_for_protocol_version(
        &self,
        protocol_version: ProtocolVersion,
    ) -> AndromedaResult<()> {
        protocol_version.validate()?;
        if !COMPLETION_ENVELOPE_VERSION.is_compatible_with(protocol_version) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "RPC completion envelope version drift against negotiated protocol version",
            ));
        }
        self.validate()
    }
}

/// Stable completion envelope contract version. Bumped only when the
/// `RpcCompletion` shape, terminal status codes, or transactional binding
/// rules change in a backwards-incompatible way.
pub const COMPLETION_ENVELOPE_VERSION: ProtocolVersion = ProtocolVersion::V1;

#[cfg(test)]
mod tests {
    use super::*;

    fn committed_template() -> RpcCompletion {
        RpcCompletion {
            request_id: Some(RequestId::new(1)),
            session_id: Some(SessionId::new(2)),
            trace_id: Some("trace".to_string()),
            status: RpcCompletionStatus::Committed,
            transaction_outcome: TransactionOutcome::Committed,
            rows_affected: Some(1),
            result_row_counts: Vec::new(),
            tx_id: Some(TransactionId::new(99)),
            durable_lsn: Some(7),
        }
    }

    #[test]
    fn rpc_completion_status_terminal_codes_match_proto_status_enum() {
        // Stable wire-aligned codes 1..=8; any change is a contract break.
        assert_eq!(RpcCompletionStatus::Committed.terminal_code(), 1);
        assert_eq!(RpcCompletionStatus::RolledBack.terminal_code(), 2);
        assert_eq!(
            RpcCompletionStatus::FailedBeforeTransaction.terminal_code(),
            3
        );
        assert_eq!(RpcCompletionStatus::Cancelled.terminal_code(), 4);
        assert_eq!(RpcCompletionStatus::Poisoned.terminal_code(), 5);
        assert_eq!(RpcCompletionStatus::PermissionDenied.terminal_code(), 6);
        assert_eq!(RpcCompletionStatus::ContractRejected.terminal_code(), 7);
        assert_eq!(RpcCompletionStatus::SystemUnavailable.terminal_code(), 8);

        for code in 1u32..=8 {
            let status = RpcCompletionStatus::from_terminal_code(code).unwrap();
            assert_eq!(status.terminal_code(), code);
        }
        assert!(RpcCompletionStatus::from_terminal_code(0).is_none());
        assert!(RpcCompletionStatus::from_terminal_code(9).is_none());

        assert!(RpcCompletionStatus::Committed.is_transactional_terminal());
        assert!(RpcCompletionStatus::RolledBack.is_transactional_terminal());
        assert!(!RpcCompletionStatus::Poisoned.is_transactional_terminal());
    }

    #[test]
    fn transaction_outcome_terminal_codes_are_stable() {
        assert_eq!(TransactionOutcome::NotStarted.terminal_code(), 1);
        assert_eq!(TransactionOutcome::Committed.terminal_code(), 2);
        assert_eq!(TransactionOutcome::RolledBack.terminal_code(), 3);
        assert_eq!(TransactionOutcome::Failed.terminal_code(), 4);
        assert_eq!(TransactionOutcome::Cancelled.terminal_code(), 5);
    }

    #[test]
    fn rpc_completion_validates_against_negotiated_protocol_version() {
        let completion = committed_template();
        // Compatible version validates.
        assert!(
            completion
                .validate_for_protocol_version(ProtocolVersion::V1)
                .is_ok()
        );

        // Major drift is rejected as a protocol-class error.
        let drift = ProtocolVersion { major: 2, minor: 0 };
        let err = completion.validate_for_protocol_version(drift).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

        // Minor drift above the envelope version is rejected.
        let minor_drift = ProtocolVersion { major: 1, minor: 1 };
        let err = completion
            .validate_for_protocol_version(minor_drift)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn poisoned_completion_with_terminal_outcome_requires_durable_lsn() {
        // Poisoned with a rolled-back outcome must carry tx id and durable
        // LSN evidence (the post-rollback poison routing invariant).
        let poisoned_no_lsn = RpcCompletion {
            status: RpcCompletionStatus::Poisoned,
            transaction_outcome: TransactionOutcome::RolledBack,
            rows_affected: Some(0),
            durable_lsn: None,
            ..committed_template()
        };
        let err = poisoned_no_lsn.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);

        let poisoned_zero_lsn = RpcCompletion {
            durable_lsn: Some(0),
            ..poisoned_no_lsn.clone()
        };
        let err = poisoned_zero_lsn.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);

        // Poisoned with rolled-back outcome and durable evidence is valid.
        let poisoned_ok = RpcCompletion {
            durable_lsn: Some(42),
            ..poisoned_no_lsn
        };
        assert!(poisoned_ok.validate().is_ok());

        // Poisoned with `NotStarted` outcome must NOT carry transaction
        // evidence (no transaction was ever begun).
        let poisoned_not_started_with_tx = RpcCompletion {
            status: RpcCompletionStatus::Poisoned,
            transaction_outcome: TransactionOutcome::NotStarted,
            rows_affected: None,
            tx_id: Some(TransactionId::new(99)),
            durable_lsn: None,
            ..committed_template()
        };
        let err = poisoned_not_started_with_tx.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Poisoned with `Failed` (non-durable) outcome must not carry a
        // durable LSN claim.
        let poisoned_failed_with_lsn = RpcCompletion {
            status: RpcCompletionStatus::Poisoned,
            transaction_outcome: TransactionOutcome::Failed,
            rows_affected: None,
            tx_id: None,
            durable_lsn: Some(7),
            ..committed_template()
        };
        let err = poisoned_failed_with_lsn.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn rpc_completion_status_matches_generated_proto_enum_values() {
        // Drift guard: the in-memory `RpcCompletionStatus::terminal_code`
        // must agree on every variant with the generated
        // `protocol::v1::rpc_completion::Status` integer values. If a new
        // proto value is added without a matching Rust variant (or vice
        // versa) this test fails as a hard contract break.
        use crate::generated::andromeda::protocol::v1::rpc_completion::Status as ProtoStatus;

        let pairs: &[(RpcCompletionStatus, ProtoStatus)] = &[
            (RpcCompletionStatus::Committed, ProtoStatus::Committed),
            (RpcCompletionStatus::RolledBack, ProtoStatus::RolledBack),
            (
                RpcCompletionStatus::FailedBeforeTransaction,
                ProtoStatus::FailedBeforeTransaction,
            ),
            (RpcCompletionStatus::Cancelled, ProtoStatus::Cancelled),
            (RpcCompletionStatus::Poisoned, ProtoStatus::Poisoned),
            (
                RpcCompletionStatus::PermissionDenied,
                ProtoStatus::PermissionDenied,
            ),
            (
                RpcCompletionStatus::ContractRejected,
                ProtoStatus::ContractRejected,
            ),
            (
                RpcCompletionStatus::SystemUnavailable,
                ProtoStatus::SystemUnavailable,
            ),
        ];

        for (rust, proto) in pairs {
            assert_eq!(
                rust.terminal_code(),
                *proto as u32,
                "RpcCompletionStatus::{:?} drifted from proto Status::{:?}",
                rust,
                proto
            );
            assert_eq!(
                RpcCompletionStatus::from_terminal_code(*proto as u32),
                Some(*rust)
            );
        }

        // Reserved unspecified slot must not round-trip into a Rust variant.
        assert_eq!(ProtoStatus::Unspecified as u32, 0);
        assert!(RpcCompletionStatus::from_terminal_code(0).is_none());
    }

    #[test]
    fn transaction_outcome_matches_generated_proto_enum_values() {
        use crate::generated::andromeda::protocol::v1::rpc_completion::TransactionOutcome as ProtoOutcome;

        let pairs: &[(TransactionOutcome, ProtoOutcome)] = &[
            (TransactionOutcome::NotStarted, ProtoOutcome::NotStarted),
            (TransactionOutcome::Committed, ProtoOutcome::Committed),
            (TransactionOutcome::RolledBack, ProtoOutcome::RolledBack),
            (TransactionOutcome::Failed, ProtoOutcome::Failed),
            (TransactionOutcome::Cancelled, ProtoOutcome::Cancelled),
        ];

        for (rust, proto) in pairs {
            assert_eq!(
                rust.terminal_code(),
                *proto as u32,
                "TransactionOutcome::{:?} drifted from proto TransactionOutcome::{:?}",
                rust,
                proto
            );
        }

        assert_eq!(ProtoOutcome::Unspecified as u32, 0);
    }

    #[test]
    fn completion_envelope_version_binds_to_locked_protocol_v1() {
        // Completion envelope contract version is locked to ProtocolVersion::V1
        // so any future bump is a deliberate breaking change visible to all
        // consumers of `validate_for_protocol_version`.
        assert_eq!(COMPLETION_ENVELOPE_VERSION, ProtocolVersion::V1);
        assert!(COMPLETION_ENVELOPE_VERSION.is_supported());
    }

    #[test]
    fn result_row_count_summary_round_trips_through_generated_proto() {
        // Cross-bind the in-memory summary to the generated proto shape so a
        // proto field rename or tag drift is caught at the contract surface.
        use crate::generated::andromeda::protocol::v1::rpc_completion::ResultRowCountSummary as ProtoSummary;

        let summary = ResultRowCountSummary {
            result_name: "rows".to_string(),
            rows_emitted: 4,
            row_count_exact: Some(4),
        };
        assert!(summary.validate().is_ok());

        let proto = ProtoSummary {
            result_name: summary.result_name.clone(),
            rows_emitted: summary.rows_emitted,
            row_count_exact: summary.row_count_exact,
        };
        assert_eq!(proto.result_name, summary.result_name);
        assert_eq!(proto.rows_emitted, summary.rows_emitted);
        assert_eq!(proto.row_count_exact, summary.row_count_exact);

        // Exact mismatch is a contract break (metadata-before-payload promise
        // about row count exactness must hold at completion as well).
        let bad = ResultRowCountSummary {
            result_name: "rows".to_string(),
            rows_emitted: 3,
            row_count_exact: Some(4),
        };
        assert_eq!(
            bad.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }
}
