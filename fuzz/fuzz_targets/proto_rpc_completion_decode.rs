#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = data.len();
    let completion = andromeda_proto::RpcCompletion {
        request_id: None,
        session_id: None,
        trace_id: None,
        status: andromeda_proto::RpcCompletionStatus::FailedBeforeTransaction,
        transaction_outcome: andromeda_proto::TransactionOutcome::NotStarted,
        rows_affected: None,
        result_row_counts: Vec::new(),
        tx_id: None,
        durable_lsn: None,
    };
    let _ = completion.validate();
});
