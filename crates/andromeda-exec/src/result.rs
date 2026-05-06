use andromeda_core::{AndromedaResult, InvocationId, RequestId, SessionId};
use andromeda_observe::{ExecutionTransitionTrace, TraceId, TransitionReasonCode};
use andromeda_srpl::Cardinality;
use andromeda_storage::Lsn;
use andromeda_tx::{TransactionState, transaction_phase_code};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultStreamMetadata {
    pub stream_id: u64,
    pub row_count_exact: Option<u64>,
    /// Optional inclusive upper bound on the number of rows the stream may emit.
    pub row_count_max: Option<u64>,
    pub column_count: u32,
    pub cardinality: Cardinality,
}

impl ResultStreamMetadata {
    /// Construct a metadata header for a stream whose row count is known
    /// exactly before payload emission.
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

    /// Construct a metadata header for a bounded stream.
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
        durable_lsn: Lsn,
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
    pub(crate) invocation_id: InvocationId,
    pub(crate) status: CompletionStatus,
    pub(crate) rows_affected: Option<u64>,
    pub(crate) transaction_state: Option<TransactionState>,
    pub(crate) durable_lsn: Option<Lsn>,
    pub(crate) trace_id: TraceId,
}

impl InvocationCompletion {
    pub fn committed(
        invocation_id: InvocationId,
        rows_affected: u64,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status: CompletionStatus::Committed,
            rows_affected: Some(rows_affected),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub fn rolled_back(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status: CompletionStatus::RolledBack,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub fn pre_transaction(
        invocation_id: InvocationId,
        status: CompletionStatus,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub const fn invocation_id(&self) -> InvocationId {
        self.invocation_id
    }

    pub const fn status(&self) -> CompletionStatus {
        self.status
    }

    pub const fn rows_affected(&self) -> Option<u64> {
        self.rows_affected
    }

    pub const fn transaction_state(&self) -> Option<TransactionState> {
        self.transaction_state
    }

    pub const fn durable_lsn(&self) -> Option<Lsn> {
        self.durable_lsn
    }

    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub fn validate(self) -> AndromedaResult<()> {
        use andromeda_core::{AndromedaError, AndromedaErrorKind};

        if self.invocation_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "invocation completion id must not be zero",
            ));
        }

        match self.status {
            CompletionStatus::Committed => {
                if self.transaction_state != Some(TransactionState::Committed) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "committed completion requires committed transaction state",
                    ));
                }
                if self.rows_affected.is_none() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Execution,
                        "committed completion requires rows affected metadata",
                    ));
                }
                let Some(durable_lsn) = self.durable_lsn else {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "committed completion requires durable WAL LSN evidence",
                    ));
                };
                if durable_lsn.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "committed completion requires nonzero durable WAL LSN evidence",
                    ));
                }
            }
            CompletionStatus::RolledBack => {
                if self.transaction_state != Some(TransactionState::RolledBack) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "rolled-back completion requires rolled-back transaction state",
                    ));
                }
                if self.rows_affected != Some(0) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Execution,
                        "rolled-back completion must report zero rows affected",
                    ));
                }
                let Some(durable_lsn) = self.durable_lsn else {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "rolled-back completion requires durable WAL LSN evidence",
                    ));
                };
                if durable_lsn.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "rolled-back completion requires nonzero durable WAL LSN evidence",
                    ));
                }
            }
            CompletionStatus::FailedBeforeTransaction
            | CompletionStatus::Cancelled
            | CompletionStatus::Poisoned
            | CompletionStatus::PermissionDenied
            | CompletionStatus::ContractRejected
            | CompletionStatus::SystemUnavailable => {
                if self.transaction_state.is_some()
                    || self.rows_affected.is_some()
                    || self.durable_lsn.is_some()
                {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Execution,
                        "non-transactional completion must not carry transaction evidence",
                    ));
                }
            }
        }

        Ok(())
    }

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
            if raw == 0 { None } else { Some(raw) }
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
    use andromeda_core::{AndromedaError, AndromedaErrorKind};

    fn require_error_kind(
        result: AndromedaResult<()>,
        context: impl Into<String>,
    ) -> AndromedaResult<AndromedaErrorKind> {
        match result {
            Ok(()) => Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                format!("expected validation error: {}", context.into()),
            )),
            Err(error) => Ok(error.kind()),
        }
    }

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
    fn exact_cardinality_requires_row_count_before_payload() -> AndromedaResult<()> {
        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            row_count_max: None,
            column_count: 2,
            cardinality: Cardinality::One,
        };

        assert_eq!(
            require_error_kind(
                metadata.validate_before_payload(),
                "exact cardinality requires row count before payload"
            )?,
            AndromedaErrorKind::Contract
        );
        Ok(())
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
    fn result_stream_completion_requires_terminal_state_and_durable_lsn() -> AndromedaResult<()> {
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
        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::Active, Lsn::new(7), 2),
                "active transaction cannot complete a result stream"
            )?,
            AndromedaErrorKind::Transaction
        );

        // Zero durable LSN cannot bind a result-stream completion.
        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::Committed, Lsn::ZERO, 2),
                "zero durable LSN cannot complete a result stream"
            )?,
            AndromedaErrorKind::Storage
        );

        // Rolled-back terminal must report zero rows.
        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::RolledBack, Lsn::new(7), 2),
                "rolled-back completion must report zero rows"
            )?,
            AndromedaErrorKind::Transaction
        );

        // Row count must match exact contract.
        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::Committed, Lsn::new(7), 3),
                "exact row count must match actual completion count"
            )?,
            AndromedaErrorKind::Contract
        );

        assert!(
            metadata
                .validate_terminal_completion(TransactionState::Committed, Lsn::new(7), 2)
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn result_metadata_enforces_declared_cardinality_bounds() -> AndromedaResult<()> {
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
                require_error_kind(
                    metadata.validate_before_payload(),
                    format!("cardinality {cardinality:?} rejects row count {row_count}")
                )?,
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
        Ok(())
    }

    #[test]
    fn result_metadata_enforces_v0_cardinality_zero_one_many_contracts() -> AndromedaResult<()> {
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
                    metadata.validate_before_payload()?;
                    metadata.validate_completed_stream(actual)?;
                }
                Some(kind) => {
                    let validation_error = match metadata.validate_before_payload() {
                        Err(error) => Some(error),
                        Ok(()) => metadata.validate_completed_stream(actual).err(),
                    };
                    let Some(error) = validation_error else {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Execution,
                            format!(
                                "expected {kind:?} for {cardinality:?} exact={exact:?} actual={actual}"
                            ),
                        ));
                    };
                    assert_eq!(error.kind(), kind);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn bounded_many_and_nonempty_many_enforce_row_count_max() -> AndromedaResult<()> {
        // Bounded Many: declared upper bound is enforced before payload and
        // at completion. The bound dominates `row_count_exact` when both are
        // declared.
        let bounded_many = ResultStreamMetadata::bounded(7, 2, Cardinality::Many, 4);
        assert!(bounded_many.validate_before_payload().is_ok());
        assert!(bounded_many.validate_completed_stream(0).is_ok());
        assert!(bounded_many.validate_completed_stream(4).is_ok());
        assert_eq!(
            require_error_kind(
                bounded_many.validate_completed_stream(5),
                "bounded many rejects row count over maximum"
            )?,
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
            require_error_kind(
                bounded_nem.validate_before_payload(),
                "nonempty many bound without exact row count"
            )?,
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
            require_error_kind(
                bad_bound.validate_before_payload(),
                "nonempty many rejects zero maximum"
            )?,
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
            require_error_kind(
                lying_one.validate_before_payload(),
                "one rejects declared maximum over one"
            )?,
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
            require_error_kind(
                exact_over_max.validate_before_payload(),
                "exact row count cannot exceed declared maximum"
            )?,
            AndromedaErrorKind::Contract
        );
        Ok(())
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
