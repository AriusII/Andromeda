use super::super::output::{
    print_backup_cancel_human, print_backup_cancel_json, print_backup_list_human,
    print_backup_list_json, print_backup_start_human, print_backup_start_json,
    print_backup_status_human, print_backup_status_json, print_backup_verify_human,
    print_backup_verify_json,
};
use super::super::{
    BackupCancelOutcome, BackupListEntry, BackupStartOutcome, BackupStatusReport,
    BackupVerifyOutcome,
};

pub(super) fn print_start(outcome: &BackupStartOutcome, json_output: bool) {
    if json_output {
        print_backup_start_json(outcome);
    } else {
        print_backup_start_human(outcome);
    }
}

pub(super) fn print_status(report: &BackupStatusReport, json_output: bool) {
    if json_output {
        print_backup_status_json(report);
    } else {
        print_backup_status_human(report);
    }
}

pub(super) fn print_list(
    backups: &[&BackupListEntry],
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
    json_output: bool,
) {
    if json_output {
        print_backup_list_json(
            backups,
            contract_preview,
            durable_backend,
            requires_storage_scheduler,
        );
    } else {
        print_backup_list_human(
            backups,
            contract_preview,
            durable_backend,
            requires_storage_scheduler,
        );
    }
}

pub(super) fn print_verify(outcome: &BackupVerifyOutcome, json_output: bool) {
    if json_output {
        print_backup_verify_json(outcome);
    } else {
        print_backup_verify_human(outcome);
    }
}

pub(super) fn print_cancel(outcome: &BackupCancelOutcome, json_output: bool) {
    if json_output {
        print_backup_cancel_json(outcome);
    } else {
        print_backup_cancel_human(outcome);
    }
}
