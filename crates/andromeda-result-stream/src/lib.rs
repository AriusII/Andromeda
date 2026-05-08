#![forbid(unsafe_code)]

mod completion;
mod metadata;
mod result_validation;
mod stream;
mod validation;

pub use completion::{COMPLETION_ENVELOPE_VERSION, CompletionStatus, InvocationCompletion};
pub use metadata::ResultStreamMetadata;
pub use result_validation::ResultValidationService;
pub use stream::{
    BackpressuredResultStream, DEFAULT_RESULT_STREAM_CAPACITY, MAX_RESULT_STREAM_CAPACITY,
    MIN_RESULT_STREAM_CAPACITY, ResultStreamMetrics, StreamCompletion,
};

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
    use andromeda_srpl_cardinality::Cardinality;

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
        use andromeda_transaction::TransactionState;
        use andromeda_wal::Lsn;

        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(2),
            row_count_max: None,
            column_count: 1,
            cardinality: Cardinality::Many,
        };

        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::Active, Lsn::new(7), 2),
                "active transaction cannot complete a result stream"
            )?,
            AndromedaErrorKind::Transaction
        );

        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::Committed, Lsn::ZERO, 2),
                "zero durable LSN cannot complete a result stream"
            )?,
            AndromedaErrorKind::Storage
        );

        assert_eq!(
            require_error_kind(
                metadata.validate_terminal_completion(TransactionState::RolledBack, Lsn::new(7), 2),
                "rolled-back completion must report zero rows"
            )?,
            AndromedaErrorKind::Transaction
        );

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
        use AndromedaErrorKind::Contract as C;
        let cases: &[(Cardinality, Option<u64>, u64, Option<AndromedaErrorKind>)] = &[
            (Cardinality::One, Some(1), 1, None),
            (Cardinality::One, Some(0), 0, Some(C)),
            (Cardinality::One, Some(2), 2, Some(C)),
            (Cardinality::One, None, 1, Some(C)),
            (Cardinality::OptionalOne, Some(0), 0, None),
            (Cardinality::OptionalOne, Some(1), 1, None),
            (Cardinality::OptionalOne, Some(2), 2, Some(C)),
            (Cardinality::OptionalOne, None, 0, None),
            (Cardinality::NonEmptyMany, Some(1), 1, None),
            (Cardinality::NonEmptyMany, Some(7), 7, None),
            (Cardinality::NonEmptyMany, Some(0), 0, Some(C)),
            (Cardinality::NonEmptyMany, None, 1, Some(C)),
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

        let bounded_nem = ResultStreamMetadata {
            stream_id: 9,
            row_count_exact: None,
            row_count_max: Some(3),
            column_count: 2,
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
        let proto_version = andromeda_proto::COMPLETION_ENVELOPE_VERSION;
        assert_eq!(COMPLETION_ENVELOPE_VERSION.0, proto_version.major);
        assert!(COMPLETION_ENVELOPE_VERSION.1 <= proto_version.minor);
    }

    #[test]
    fn completion_status_terminal_codes_match_proto_status_codes() {
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
                "CompletionStatus::{:?} drifted from RpcCompletionStatus::{:?}",
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
