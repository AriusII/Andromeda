use andromeda_error::AndromedaResult;
use andromeda_types::{ContractHash, RequestId, SessionId};

use crate::events::{
    EventEnvelope, ProtocolEventScope, TraceEvent, TransactionPhaseCode, TransitionReasonCode,
    WalOperation, observe_error,
};

pub(super) fn validate(envelope: &EventEnvelope) -> AndromedaResult<()> {
    validate_values(envelope)?;
    validate_protocol_scope(envelope)?;
    validate_request_session(envelope)?;
    validate_denied_path(envelope)?;
    validate_gpu_execution(envelope)?;
    validate_transaction(envelope)?;
    validate_catalog(envelope)
}

fn validate_values(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if envelope
        .correlation
        .request_id
        .is_some_and(|request_id| request_id.get() == 0)
    {
        return Err(observe_error(
            "observability request_id correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .session_id
        .is_some_and(|session_id| session_id.get() == 0)
    {
        return Err(observe_error(
            "observability session_id correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .contract_hash
        .is_some_and(ContractHash::is_zero)
    {
        return Err(observe_error(
            "observability contract_hash correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .catalog_version
        .is_some_and(|catalog_version| catalog_version.get() == 0)
    {
        return Err(observe_error(
            "observability catalog_version correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .catalog_object_id
        .is_some_and(|catalog_object_id| catalog_object_id.get() == 0)
    {
        return Err(observe_error(
            "observability catalog_object_id correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .transaction_id
        .is_some_and(|transaction_id| transaction_id.get() == 0)
    {
        return Err(observe_error(
            "observability transaction_id correlation must be non-zero when present",
        ));
    }

    if envelope
        .correlation
        .durable_lsn
        .is_some_and(|durable_lsn| durable_lsn == 0)
    {
        return Err(observe_error(
            "observability durable_lsn correlation must be non-zero when present",
        ));
    }

    Ok(())
}

fn validate_request_session(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if envelope.event.requires_request_session_correlation()
        && !envelope.correlation.has_request_session()
    {
        return Err(observe_error(
            "request-scoped security and protocol events require non-zero request_id and session_id correlation",
        ));
    }

    Ok(())
}

fn validate_denied_path(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if envelope.event.must_not_have_transaction_correlation()
        && !envelope.correlation.has_no_transaction_evidence()
    {
        return Err(observe_error(
            "denied pre-transaction paths must not include transaction or durable LSN correlation",
        ));
    }

    Ok(())
}

fn validate_transaction(envelope: &EventEnvelope) -> AndromedaResult<()> {
    match &envelope.event {
        TraceEvent::WalEvent(trace) => {
            match trace.transaction_id {
                Some(transaction_id)
                    if envelope.correlation.transaction_id != Some(transaction_id) =>
                {
                    return Err(observe_error(
                        "WAL event transaction_id correlation must match WAL trace payload",
                    ));
                },
                _ => {},
            }

            if trace.operation == WalOperation::Flush
                && envelope.correlation.durable_lsn != trace.durable_lsn
            {
                return Err(observe_error(
                    "WAL flush durable_lsn correlation must match WAL trace payload",
                ));
            }
        },
        TraceEvent::CommitVisible(trace)
            if envelope.correlation.transaction_id != Some(trace.transaction_id)
                || envelope.correlation.durable_lsn != Some(trace.durable_commit_lsn) =>
        {
            return Err(observe_error(
                "commit-visible traces require matching transaction_id and durable_lsn correlation",
            ));
        },
        TraceEvent::RollbackDurable(trace)
            if envelope.correlation.transaction_id != Some(trace.transaction_id)
                || envelope.correlation.durable_lsn != Some(trace.durable_rollback_lsn) =>
        {
            return Err(observe_error(
                "rollback-durable traces require matching transaction_id and durable_lsn correlation",
            ));
        },
        TraceEvent::RecoveryStartup(trace)
            if envelope.correlation.durable_lsn != Some(trace.last_durable_lsn) =>
        {
            return Err(observe_error(
                "recovery startup traces require matching durable_lsn correlation",
            ));
        },
        TraceEvent::CompletionEmitted(trace)
            if trace.committed && envelope.correlation.durable_lsn != trace.durable_lsn =>
        {
            return Err(observe_error(
                "committed completion traces require matching durable_lsn correlation",
            ));
        },
        TraceEvent::TransactionTransition(trace) => {
            if envelope.correlation.transaction_id != Some(trace.transaction_id) {
                return Err(observe_error(
                    "transaction transition traces require matching transaction_id correlation",
                ));
            }
            validate_payload_durable_lsn(
                envelope,
                trace.durable_lsn,
                trace.next_phase.requires_durable_evidence(),
                "transaction transition traces",
            )?;
            validate_payload_request_session(
                envelope,
                trace.request_id,
                trace.session_id,
                "transaction transition traces",
            )?;
        },
        TraceEvent::ExecutionTransition(trace) => {
            match trace.transaction_id {
                Some(payload_tx) if envelope.correlation.transaction_id != Some(payload_tx) => {
                    return Err(observe_error(
                        "execution transition traces require matching transaction_id correlation when payload carries one",
                    ));
                },
                _ => {},
            }
            validate_payload_durable_lsn(
                envelope,
                trace.durable_lsn,
                trace
                    .next_phase
                    .is_some_and(TransactionPhaseCode::requires_durable_evidence),
                "execution transition traces",
            )?;
            let denied = matches!(
                trace.reason_code,
                TransitionReasonCode::PERMISSION_DENIED
                    | TransitionReasonCode::PRE_TRANSACTION_REJECTION
            );
            if denied && !envelope.correlation.has_no_transaction_evidence() {
                return Err(observe_error(
                    "execution transition traces with a pre-transaction rejection reason must not carry transaction or durable_lsn correlation",
                ));
            }
            validate_payload_request_session(
                envelope,
                trace.request_id,
                trace.session_id,
                "execution transition traces",
            )?;
        },
        _ => {},
    }

    Ok(())
}

fn validate_gpu_execution(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if !matches!(envelope.event, TraceEvent::GpuExecution(_)) {
        return Ok(());
    }

    if !envelope.correlation.has_no_transaction_evidence() {
        return Err(observe_error(
            "GPU execution traces must not include transaction or durable LSN correlation",
        ));
    }

    if envelope.correlation.protocol.is_some() {
        return Err(observe_error(
            "GPU execution traces must not include protocol correlation",
        ));
    }

    if envelope.correlation.request_id.is_some() != envelope.correlation.session_id.is_some() {
        return Err(observe_error(
            "GPU execution traces require request_id and session_id correlation to be paired",
        ));
    }

    if envelope.correlation.contract_hash.is_some()
        != envelope.correlation.catalog_version.is_some()
    {
        return Err(observe_error(
            "GPU execution traces require contract_hash and catalog_version correlation to be paired",
        ));
    }

    Ok(())
}

fn validate_payload_durable_lsn(
    envelope: &EventEnvelope,
    durable_lsn: Option<u64>,
    terminal_requires_lsn: bool,
    context: &str,
) -> AndromedaResult<()> {
    if let Some(payload_lsn) = durable_lsn {
        if envelope.correlation.durable_lsn != Some(payload_lsn) {
            return Err(observe_error(format!(
                "{context} require matching durable_lsn correlation when payload carries one",
            )));
        }
    } else if terminal_requires_lsn {
        return Err(observe_error(format!(
            "terminal {context} require durable_lsn correlation",
        )));
    }

    Ok(())
}

fn validate_payload_request_session(
    envelope: &EventEnvelope,
    request_id: Option<RequestId>,
    session_id: Option<SessionId>,
    context: &str,
) -> AndromedaResult<()> {
    match request_id {
        Some(request_id) if envelope.correlation.request_id != Some(request_id) => {
            return Err(observe_error(format!(
                "{context} require matching request_id correlation when payload carries one",
            )));
        },
        _ => {},
    }
    match session_id {
        Some(session_id) if envelope.correlation.session_id != Some(session_id) => {
            return Err(observe_error(format!(
                "{context} require matching session_id correlation when payload carries one",
            )));
        },
        _ => {},
    }

    Ok(())
}

fn validate_catalog(envelope: &EventEnvelope) -> AndromedaResult<()> {
    match &envelope.event {
        TraceEvent::Manifest(trace)
            if envelope.correlation.catalog_version != Some(trace.catalog_version) =>
        {
            return Err(observe_error(
                "manifest traces require matching catalog_version correlation",
            ));
        },
        _ => {},
    }

    Ok(())
}

fn validate_protocol_scope(envelope: &EventEnvelope) -> AndromedaResult<()> {
    match envelope.event.protocol_scope() {
        Some(ProtocolEventScope::Request) => {
            let has_request_id = envelope
                .correlation
                .request_id
                .is_some_and(|request_id| request_id.get() != 0);
            let has_session_id = envelope
                .correlation
                .session_id
                .is_some_and(|session_id| session_id.get() != 0);

            if !has_request_id || !has_session_id {
                return Err(observe_error(
                    "request-scoped protocol events require non-zero request_id and session_id correlation",
                ));
            }

            Ok(())
        },
        Some(ProtocolEventScope::Session) => {
            let has_session_id = envelope
                .correlation
                .session_id
                .is_some_and(|session_id| session_id.get() != 0);

            if !has_session_id {
                return Err(observe_error(
                    "session-scoped protocol events require non-zero session_id correlation",
                ));
            }

            Ok(())
        },
        Some(ProtocolEventScope::Connection) | None => Ok(()),
    }
}
