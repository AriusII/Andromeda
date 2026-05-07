mod format;
mod help;
mod human;
mod json;
mod labels;
mod records;

use super::{AuditCompactReport, AuditInspectionReport, AuditVerifyReport};

pub(super) fn print_audit_help() {
    help::print_audit_help();
}

pub(super) fn print_audit_inspection_result(
    report: &AuditInspectionReport,
    json_output: bool,
    _diagnostic_json: bool,
) {
    if json_output {
        json::print_audit_inspection_json(report);
    } else {
        human::print_audit_inspection_human(report);
    }
}

pub(super) fn print_audit_compact_result(report: &AuditCompactReport, diagnostic_json: bool) {
    if diagnostic_json {
        json::print_audit_compact_json(report);
    } else {
        human::print_audit_compact_human(report);
    }
}

pub(super) fn print_audit_verify_result(report: &AuditVerifyReport, json_output: bool) {
    if json_output {
        json::print_audit_verify_json(report);
    } else {
        human::print_audit_verify_human(report);
    }
}
