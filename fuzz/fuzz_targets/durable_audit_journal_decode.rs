#![no_main]

use libfuzzer_sys::fuzz_target;

mod common;
mod durable_audit_journal_support;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, durable_audit_journal_support::MAX_TOTAL_BYTES);
    let path = durable_audit_journal_support::unique_journal_path();
    durable_audit_journal_support::cleanup_journal_files(&path);

    durable_audit_journal_support::write_fuzz_journal(&path, data);
    durable_audit_journal_support::exercise_replay(&path, data);

    durable_audit_journal_support::cleanup_journal_files(&path);
});
