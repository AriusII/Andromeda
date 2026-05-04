use andromeda_core::{AndromedaResult, InvocationId, RequestId, SessionId};
use andromeda_observe::{ExecutionTransitionTrace, TraceId, TransitionReasonCode};
use andromeda_srpl::Cardinality;
use andromeda_storage::Lsn;
use andromeda_tx::{transaction_phase_code, TransactionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultStreamMetadata {
    pub stream_id: u64,
    pub row_count_exact: Option<u64>,
    /// Optional inclusive upper bound on the number of rows the stream may
    /// emit. Combined with `cardinality`, this is the V0 mechanism for
    /// declaring a *bounded* `Many` or `NonEmptyMany` result without
    /// inventing a streaming engine. For `One`/`OptionalOne` the bound, when
    /// declared, must agree with the intrinsic max of 1.
    pub row_count_max: Option<u64>,
    pub column_count: u32,
    pub cardinality: Cardinality,
}

impl ResultStreamMetadata {
    /// Construct a metadata header for a stream whose row count is known
    /// exactly before payload emission. The `row_count_max` is set to the
    /// exact count so downstream framers can treat the bound uniformly.
    pub const fn exact(
        stream_id: u64,
        column_count: u32,
        cardinality: Cardinality,
        row_count_exact: u64,
    ) -> Self {
        Self {
            stream_id,
            row_count_exact: Some(row_count_exact),
            row_count_max: Some(row_count_exact),
            column_count,
            cardinality,
        }
    }

    /// Construct a metadata header for a `Many` / `NonEmptyMany` stream
    /// whose exact row count is not known up front but whose upper bound
    /// is contractual.
    pub const fn bounded(
        stream_id: u64,
        column_count: u32,
        cardinality: Cardinality,
        row_count_max: u64,
    ) -> Self {
        Self {
            stream_id,
            row_count_exact: None,
            row_count_max: Some(row_count_max),
            column_count,
            cardinality,
        }
    }

    pub fn validate_before_payload(self) -> AndromedaResult<()> {
        crate::services::ResultValidationService::validate_before_payload(self)
    }

    pub fn validate_completed_stream(self, actual_row_count: u64) -> AndromedaResult<()> {
        crate::services::ResultValidationService::validate_completed_stream(self, actual_row_count)
    }

    /// Bind the terminal completion of this result stream to a transaction
    /// terminal state and durable LSN evidence. Use this at the
    /// completion-emission boundary so a result-stream completion can never
    /// be emitted ahead of (or without) the transaction reaching a terminal
    /// state with durable WAL evidence. The metadata-before-payload contract
    /// and exact row count contract are enforced in the same pass.
    pub fn validate_terminal_completion(
        self,
        transaction_state: andromeda_tx::TransactionState,
        durable_lsn: andromeda_storage::Lsn,
        actual_row_count: u64,
    ) -> AndromedaResult<()> {
        use andromeda_core::{AndromedaError, AndromedaErrorKind};
        use andromeda_tx::TransactionState;

        if !matches!(
            transaction_state,
            TransactionState::Committed | TransactionState::RolledBack
        ) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "result stream completion requires a terminal transaction state",
            ));
        }

        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "result stream completion requires nonzero durable LSN evidence",
            ));
        }

        if transaction_state == TransactionState::RolledBack && actual_row_count != 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rolled-back result stream completion must report zero rows",
            ));
        }

        self.validate_completed_stream(actual_row_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionStatus {
    Committed,
    RolledBack,
    FailedBeforeTransaction,
    Cancelled,
    Poisoned,
    PermissionDenied,
    ContractRejected,
    SystemUnavailable,
}

impl CompletionStatus {
    /// Stable terminal completion code aligned with the wire-level
    /// `andromeda_proto::RpcCompletionStatus::terminal_code` so executor
    /// telemetry, journals, and recovery comparisons never depend on
    /// `Debug`/`Display` projections.
    ///
    /// Codes match `protocol::v1::rpc_completion::Status` values 1..=8;
    /// `0` (`STATUS_UNSPECIFIED`) is intentionally unreachable from this enum.
    pub const fn terminal_code(self) -> u32 {
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

    /// Returns true when the status reports a transaction that reached a
    /// durable terminal state (committed or rolled back). Pre-transaction
    /// rejections, cancellations, and resource failures return false.
    pub const fn is_transactional_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }

    /// Maps the completion status onto the stable
    /// [`TransitionReasonCode`] used by execution-side transition traces so
    /// observability sinks can route by reason without re-deriving it.
    pub const fn transition_reason_code(self) -> TransitionReasonCode {
        match self {
            Self::Committed | Self::RolledBack => TransitionReasonCode::DURABLE_WAL_FLUSH,
            Self::FailedBeforeTransaction => TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            Self::Cancelled => TransitionReasonCode::CANCELLED,
            Self::Poisoned => TransitionReasonCode::POISON,
            Self::PermissionDenied => TransitionReasonCode::PERMISSION_DENIED,
            Self::ContractRejected => TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            Self::SystemUnavailable => TransitionReasonCode::SYSTEM_UNAVAILABLE,
        }
    }
}

/// Completion envelope contract version owned by the executor. Bumped only
/// when the in-memory `InvocationCompletion` shape, terminal codes, or
/// transactional binding rules change in a backwards-incompatible way.
/// Major mirrors `andromeda_proto::COMPLETION_ENVELOPE_VERSION`; this is
/// kept as a plain `(u32, u32)` to avoid a hard re-export dependency.
pub const COMPLETION_ENVELOPE_VERSION: (u32, u32) = (1, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationCompletion {
    pub invocation_id: InvocationId,
    pub status: CompletionStatus,
    pub rows_affected: Option<u64>,
    pub transaction_state: Option<TransactionState>,
    pub durable_lsn: Option<Lsn>,
    pub trace_id: TraceId,
}

impl InvocationCompletion {
    /// Project a terminal `InvocationCompletion` into an
    /// [`ExecutionTransitionTrace`] carrying stable invocation/request/
    /// session/transaction correlation. Use this at the completion-emission
    /// boundary so every terminal completion produces auditable transition
    /// evidence, not just sink-routed `CompletionEmittedTrace` payloads.
    ///
    /// `previous_state` is the transaction phase the invocation was in
    /// before reaching the recorded terminal state. Pass `None` for
    /// pre-transaction terminations (e.g. `FailedBeforeTransaction`,
    /// `PermissionDenied`, `ContractRejected`) where no transaction ever
    /// existed.
    pub fn project_transition(
        &self,
        previous_state: Option<TransactionState>,
        request_id: Option<RequestId>,
        session_id: Option<SessionId>,
        transaction_id: Option<andromeda_core::TransactionId>,
        reason: impl Into<String>,
    ) -> ExecutionTransitionTrace {
        let next_phase = self.transaction_state.map(transaction_phase_code);
        let prev_phase = previous_state.map(transaction_phase_code);
        let durable_lsn = self.durable_lsn.and_then(|lsn| {
            let raw = lsn.get();
            if raw == 0 {
                None
            } else {
                Some(raw)
            }
        });
        // Pre-transaction rejection paths must not carry transaction or
        // durable LSN evidence. We strip them here defensively so a misuse
        // upstream does not fabricate durability claims, and `validate()`
        // re-checks the same invariant.
        let denied = matches!(
            self.status,
            CompletionStatus::PermissionDenied
                | CompletionStatus::ContractRejected
                | CompletionStatus::FailedBeforeTransaction
        );
        let (transaction_id, durable_lsn) = if denied {
            (None, None)
        } else {
            (transaction_id, durable_lsn)
        };
        ExecutionTransitionTrace {
            trace_id: self.trace_id,
            invocation_id: self.invocation_id,
            request_id,
            session_id,
            transaction_id,
            completion_code: Some(self.status.terminal_code()),
            prev_phase,
            next_phase,
            durable_lsn,
            reason_code: self.status.transition_reason_code(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::AndromedaErrorKind;

    #[test]
    fn result_metadata_keeps_shape_before_payload_contract() {
        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(3),
            row_count_max: None,
            column_count: 2,
            cardinality: Cardinality::NonEmptyMany,
        };

        assert!(metadata.validate_before_payload().is_ok());
    }

    #[test]
    fn exact_cardinality_requires_row_count_before_payload() {
        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            row_count_max: None,
            column_count: 2,
            cardinality: Cardinality::One,
        };

        assert_eq!(
            metadata.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn completion_status_terminal_codes_are_stable_and_match_proto() {
        // Stable wire-aligned codes 1..=8 matching
        // protocol::v1::rpc_completion::Status. Any change here is a
        // breaking change to the completion envelope contract.
        assert_eq!(CompletionStatus::Committed.terminal_code(), 1);
        assert_eq!(CompletionStatus::RolledBack.terminal_code(), 2);
        assert_eq!(CompletionStatus::FailedBeforeTransaction.terminal_code(), 3);
        assert_eq!(CompletionStatus::Cancelled.terminal_code(), 4);
        assert_eq!(CompletionStatus::Poisoned.terminal_code(), 5);
        assert_eq!(CompletionStatus::PermissionDenied.terminal_code(), 6);
        assert_eq!(CompletionStatus::ContractRejected.terminal_code(), 7);
        assert_eq!(CompletionStatus::SystemUnavailable.terminal_code(), 8);

        assert!(CompletionStatus::Committed.is_transactional_terminal());
        assert!(CompletionStatus::RolledBack.is_transactional_terminal());
        assert!(!CompletionStatus::Poisoned.is_transactional_terminal());
        assert!(!CompletionStatus::FailedBeforeTransaction.is_transactional_terminal());
    }

    #[test]
    fn result_stream_completion_requires_terminal_transaction_state_and_durable_lsn() {
        use andromeda_storage::Lsn;
        use andromeda_tx::TransactionState;

        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(2),
            row_count_max: None,
            column_count: 1,
            cardinality: Cardinality::Many,
        };

        // Non-terminal transaction state cannot bind a result-stream completion.
        let err = metadata
            .validate_terminal_completion(TransactionState::Active, Lsn::new(7), 2)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Zero durable LSN cannot bind a result-stream completion.
        let err = metadata
            .validate_terminal_completion(TransactionState::Committed, Lsn::ZERO, 2)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);

        // Rolled-back terminal must report zero rows.
        let err = metadata
            .validate_terminal_completion(TransactionState::RolledBack, Lsn::new(7), 2)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Row count must match exact contract.
        let err = metadata
            .validate_terminal_completion(TransactionState::Committed, Lsn::new(7), 3)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);

        assert!(metadata
            .validate_terminal_completion(TransactionState::Committed, Lsn::new(7), 2)
            .is_ok());
    }

    #[test]
    fn result_metadata_enforces_declared_cardinality_bounds() {
        for (cardinality, row_count) in [
            (Cardinality::One, 0),
            (Cardinality::One, 2),
            (Cardinality::OptionalOne, 2),
            (Cardinality::NonEmptyMany, 0),
        ] {
            let metadata = ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(row_count),
                row_count_max: None,
                column_count: 2,
                cardinality,
            };

            assert_eq!(
                metadata.validate_before_payload().unwrap_err().kind(),
                AndromedaErrorKind::Contract
            );
        }

        let many = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            row_count_max: None,
            column_count: 2,
            cardinality: Cardinality::Many,
        };
        assert!(many.validate_before_payload().is_ok());
    }

    #[test]
    fn result_metadata_enforces_v0_cardinality_zero_one_many_contracts() {
        // V0 cardinality contract — exhaustive 0/1/>1 behaviour per kind.
        // Each row encodes (cardinality, declared_exact, actual_at_completion,
        // expected_outcome) where outcome is Ok or the AndromedaErrorKind it
        // must reject under.
        use AndromedaErrorKind::Contract as C;
        let cases: &[(Cardinality, Option<u64>, u64, Option<AndromedaErrorKind>)] = &[
            // One: requires exact, must equal 1.
            (Cardinality::One, Some(1), 1, None),
            (Cardinality::One, Some(0), 0, Some(C)),
            (Cardinality::One, Some(2), 2, Some(C)),
            (Cardinality::One, None, 1, Some(C)),
            // OptionalOne: requires exact, must be 0 or 1.
            (Cardinality::OptionalOne, Some(0), 0, None),
            (Cardinality::OptionalOne, Some(1), 1, None),
            (Cardinality::OptionalOne, Some(2), 2, Some(C)),
            (Cardinality::OptionalOne, None, 0, Some(C)),
            // NonEmptyMany: requires exact, must be >= 1.
            (Cardinality::NonEmptyMany, Some(1), 1, None),
            (Cardinality::NonEmptyMany, Some(7), 7, None),
            (Cardinality::NonEmptyMany, Some(0), 0, Some(C)),
            (Cardinality::NonEmptyMany, None, 1, Some(C)),
            // Many: exact optional; any nonneg actual permitted.
            (Cardinality::Many, None, 0, None),
            (Cardinality::Many, None, 99, None),
            (Cardinality::Many, Some(3), 3, None),
        ];
        for &(cardinality, exact, actual, expected_err) in cases {
            let metadata = ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: exact,
                row_count_max: None,
                column_count: 1,
                cardinality,
            };
            match expected_err {
                None => {
                    metadata.validate_before_payload().unwrap_or_else(|e| {
                        panic!("pre-payload {:?} {:?}: {e:?}", cardinality, exact)
                    });
                    metadata
                        .validate_completed_stream(actual)
                        .unwrap_or_else(|e| {
                            panic!(
                                "completed {:?} {:?} actual={actual}: {e:?}",
                                cardinality, exact
                            )
                        });
                }
                Some(kind) => {
                    let err = metadata
                        .validate_before_payload()
                        .err()
                        .or_else(|| metadata.validate_completed_stream(actual).err())
                        .unwrap_or_else(|| {
                            panic!("expected error {kind:?} for {:?} {:?}", cardinality, exact)
                        });
                    assert_eq!(err.kind(), kind);
                }
            }
        }
    }

    #[test]
    fn bounded_many_and_nonempty_many_enforce_row_count_max() {
        // Bounded Many: declared upper bound is enforced before payload and
        // at completion. The bound dominates `row_count_exact` when both are
        // declared.
        let bounded_many = ResultStreamMetadata::bounded(7, 2, Cardinality::Many, 4);
        assert!(bounded_many.validate_before_payload().is_ok());
        assert!(bounded_many.validate_completed_stream(0).is_ok());
        assert!(bounded_many.validate_completed_stream(4).is_ok());
        assert_eq!(
            bounded_many
                .validate_completed_stream(5)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );

        // Bounded NonEmptyMany: bound must be >= 1, actual must be 1..=bound.
        let bounded_nem = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: None,
            row_count_max: Some(3),
            column_count: 2,
            // NonEmptyMany usually requires `row_count_exact`; the bound on
            // its own is not a substitute, so before-payload must reject.
            cardinality: Cardinality::NonEmptyMany,
        };
        assert_eq!(
            bounded_nem.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let bounded_nem_exact = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: Some(2),
            row_count_max: Some(3),
            column_count: 2,
            cardinality: Cardinality::NonEmptyMany,
        };
        assert!(bounded_nem_exact.validate_before_payload().is_ok());
        assert!(bounded_nem_exact.validate_completed_stream(2).is_ok());

        // row_count_max < cardinality.min → reject as Contract.
        let bad_bound = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: None,
            row_count_max: Some(0),
            column_count: 1,
            cardinality: Cardinality::NonEmptyMany,
        };
        assert_eq!(
            bad_bound.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        // One/OptionalOne intrinsic max is 1: a declared bound > 1 is
        // contradictory metadata and must be rejected before payload.
        let lying_one = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: Some(1),
            row_count_max: Some(2),
            column_count: 1,
            cardinality: Cardinality::One,
        };
        assert_eq!(
            lying_one.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        // exact > max is inconsistent metadata and must be rejected.
        let exact_over_max = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: Some(5),
            row_count_max: Some(3),
            column_count: 1,
            cardinality: Cardinality::Many,
        };
        assert_eq!(
            exact_over_max.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn result_metadata_constructors_emit_consistent_bounds() {
        let exact = ResultStreamMetadata::exact(3, 2, Cardinality::One, 1);
        assert_eq!(exact.row_count_exact, Some(1));
        assert_eq!(exact.row_count_max, Some(1));
        assert!(exact.validate_before_payload().is_ok());
        assert!(exact.validate_completed_stream(1).is_ok());

        let bounded = ResultStreamMetadata::bounded(3, 2, Cardinality::Many, 16);
        assert_eq!(bounded.row_count_exact, None);
        assert_eq!(bounded.row_count_max, Some(16));
        assert!(bounded.validate_before_payload().is_ok());
    }

    #[test]
    fn completion_envelope_version_major_matches_proto_contract() {
        // Cross-bind: the executor's in-memory completion envelope version
        // must agree on the major axis with the wire-level
        // `andromeda_proto::COMPLETION_ENVELOPE_VERSION`. Drift here means a
        // breaking executor change shipped without bumping the wire contract
        // (or vice versa).
        let proto_version = andromeda_proto::COMPLETION_ENVELOPE_VERSION;
        assert_eq!(COMPLETION_ENVELOPE_VERSION.0, proto_version.major);
        assert!(COMPLETION_ENVELOPE_VERSION.1 <= proto_version.minor);
    }

    #[test]
    fn completion_status_terminal_codes_match_proto_status_codes() {
        // Wire-aligned drift guard between
        // `andromeda_exec::result::CompletionStatus` and
        // `andromeda_proto::RpcCompletionStatus`. The two enums are
        // intentionally separate to keep the executor independent of wire
        // codec types, but their terminal codes must agree by construction.
        use andromeda_proto::RpcCompletionStatus as Proto;

        let pairs: &[(CompletionStatus, Proto)] = &[
            (CompletionStatus::Committed, Proto::Committed),
            (CompletionStatus::RolledBack, Proto::RolledBack),
            (
                CompletionStatus::FailedBeforeTransaction,
                Proto::FailedBeforeTransaction,
            ),
            (CompletionStatus::Cancelled, Proto::Cancelled),
            (CompletionStatus::Poisoned, Proto::Poisoned),
            (CompletionStatus::PermissionDenied, Proto::PermissionDenied),
            (CompletionStatus::ContractRejected, Proto::ContractRejected),
            (
                CompletionStatus::SystemUnavailable,
                Proto::SystemUnavailable,
            ),
        ];

        for (exec, proto) in pairs {
            assert_eq!(
                exec.terminal_code(),
                proto.terminal_code(),
                "exec CompletionStatus::{:?} drifted from proto RpcCompletionStatus::{:?}",
                exec,
                proto
            );
            assert_eq!(
                exec.is_transactional_terminal(),
                proto.is_transactional_terminal()
            );
        }
    }
}
