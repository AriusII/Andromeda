use andromeda_audit::DurableAuditTraceQueryRow;

use super::{
    format::{audit_output_text, json_audit_string, json_option_audit_string, json_raw_string},
    labels::{
        durable_family_str, durable_trace_family_str, permission_str, replay_behavior_str,
        retention_str, surface_str,
    },
};

pub(super) fn print_trace_row_human(row: &DurableAuditTraceQueryRow) {
    println!(
        "- event_id={} trace_id={} family={} record_lsn={} principal={} event_kind={}",
        row.event_id.get(),
        row.trace_id.get(),
        durable_trace_family_str(row.family),
        row.record_lsn,
        audit_output_text(&row.principal_id),
        audit_output_text(&row.event_kind)
    );
}

pub(super) fn rows_json(rows: &[DurableAuditTraceQueryRow]) -> String {
    let entries = rows.iter().map(row_json).collect::<Vec<_>>().join(",");
    format!("[{}]", entries)
}

fn row_json(row: &DurableAuditTraceQueryRow) -> String {
    format!(
        "{{\"event_id\":{},\"trace_id\":{},\"family\":{},\"durable_audit_family\":{},\"sequence_number\":{},\"record_lsn\":{},\"durable_lsn\":{},\"checksum\":{},\"replay_behavior\":{},\"retention\":{},\"principal_id\":{},\"certificate_fingerprint\":{},\"surface\":{},\"permission\":{},\"request_id\":{},\"session_id\":{},\"event_kind\":{}}}",
        row.event_id.get(),
        row.trace_id.get(),
        json_raw_string(durable_trace_family_str(row.family)),
        json_raw_string(durable_family_str(row.durable_audit_family)),
        row.sequence_number,
        row.record_lsn,
        row.durable_lsn,
        row.checksum,
        json_raw_string(replay_behavior_str(row.replay_behavior)),
        json_raw_string(retention_str(row.retention)),
        json_option_audit_string(Some(&row.principal_id)),
        json_option_audit_string(row.certificate_fingerprint.as_deref()),
        row.surface
            .map(|surface| json_raw_string(surface_str(surface)))
            .unwrap_or_else(|| "null".to_string()),
        row.permission
            .map(|permission| json_raw_string(permission_str(permission)))
            .unwrap_or_else(|| "null".to_string()),
        row.request_id
            .map(|request_id| request_id.get().to_string())
            .unwrap_or_else(|| "null".to_string()),
        row.session_id
            .map(|session_id| session_id.get().to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_audit_string(&row.event_kind),
    )
}
