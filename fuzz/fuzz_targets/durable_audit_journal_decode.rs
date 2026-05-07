#![no_main]

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditReplayLsnRange, DurableAuditReplayQuery,
    DurableAuditReplayWindow, FileDurableAuditWalSink, TraceId,
};
use libfuzzer_sys::fuzz_target;

const MAX_JOURNAL_BYTES: usize = 64 * 1024;
const MAX_ANCHOR_BYTES: usize = 4 * 1024;
const MAX_TOTAL_BYTES: usize = MAX_JOURNAL_BYTES + MAX_ANCHOR_BYTES + 3;
const MAX_RECORDS_TO_VALIDATE: usize = 32;
const MAX_REPLAY_WINDOW: usize = 16;

static CASE_COUNTER: AtomicU64 = AtomicU64::new(0);

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_TOTAL_BYTES)];
    let path = unique_journal_path();
    cleanup_journal_files(&path);

    write_fuzz_journal(&path, data);
    exercise_replay(&path, data);

    cleanup_journal_files(&path);
});

fn write_fuzz_journal(path: &Path, data: &[u8]) {
    let Some((&mode, rest)) = data.split_first() else {
        let _ = fs::write(path, []);
        return;
    };

    if mode & 1 == 0 {
        let journal_len = rest.len().min(MAX_JOURNAL_BYTES);
        let _ = fs::write(path, &rest[..journal_len]);
        return;
    }

    let (split, payload) = split_hint(rest);
    let split = split.min(payload.len());
    let journal_len = split.min(MAX_JOURNAL_BYTES);
    let anchor_len = payload[split..].len().min(MAX_ANCHOR_BYTES);
    let _ = fs::write(path, &payload[..journal_len]);
    let _ = fs::write(anchor_path(path), &payload[split..split + anchor_len]);
}

fn split_hint(data: &[u8]) -> (usize, &[u8]) {
    if data.len() < 2 {
        return (0, data);
    }
    let payload = &data[2..];
    if payload.is_empty() {
        return (0, payload);
    }
    let raw_split = u16::from_le_bytes([data[0], data[1]]) as usize;
    (raw_split % (payload.len() + 1), payload)
}

fn exercise_replay(path: &Path, data: &[u8]) {
    let Ok(sink) = FileDurableAuditWalSink::open(path) else {
        return;
    };

    let _ = sink.replay(&DurableAuditReplayQuery::all());
    let query = replay_query(data);
    let window = replay_window(data);
    let Ok(result) = sink.query_with_evidence(&query, window) else {
        return;
    };

    assert_eq!(result.evidence.records_returned, result.records.len());
    assert!(result.evidence.records_scanned >= result.evidence.records_matched);
    assert!(result.evidence.records_matched >= result.evidence.records_returned);
    assert!(result.evidence.records_returned <= window.limit);

    for record in result.records.iter().take(MAX_RECORDS_TO_VALIDATE) {
        assert!(record.validate().is_ok());
        assert!(record.matches_query(&query));
    }
}

fn replay_query(data: &[u8]) -> DurableAuditReplayQuery {
    match byte_at(data, 3) % 5 {
        0 => DurableAuditReplayQuery::all(),
        1 => DurableAuditReplayQuery {
            family: Some(family_from_byte(byte_at(data, 4))),
            ..DurableAuditReplayQuery::all()
        },
        2 => DurableAuditReplayQuery {
            trace_id: Some(TraceId::new(nonzero_u64(data, 5) as u128)),
            ..DurableAuditReplayQuery::all()
        },
        3 => {
            let start = nonzero_u64(data, 13);
            let width = u64::from(byte_at(data, 21)) + 1;
            DurableAuditReplayQuery {
                lsn_range: Some(DurableAuditReplayLsnRange::new(
                    start,
                    start.saturating_add(width),
                )),
                ..DurableAuditReplayQuery::all()
            }
        }
        _ => DurableAuditReplayQuery {
            principal_id: Some("fuzz-principal".to_string()),
            ..DurableAuditReplayQuery::all()
        },
    }
}

fn replay_window(data: &[u8]) -> DurableAuditReplayWindow {
    let limit = usize::from(byte_at(data, 22) % MAX_REPLAY_WINDOW as u8) + 1;
    let offset = usize::from(byte_at(data, 23) % MAX_REPLAY_WINDOW as u8);
    DurableAuditReplayWindow::new(limit, offset)
}

const fn family_from_byte(byte: u8) -> DurableAuditEventFamily {
    match byte % 10 {
        0 => DurableAuditEventFamily::SecurityDecision,
        1 => DurableAuditEventFamily::AdminDecision,
        2 => DurableAuditEventFamily::AdmissionDecision,
        3 => DurableAuditEventFamily::CatalogDecision,
        4 => DurableAuditEventFamily::HadrDecision,
        5 => DurableAuditEventFamily::BackupDecision,
        6 => DurableAuditEventFamily::RestoreDecision,
        7 => DurableAuditEventFamily::ForensicDecision,
        8 => DurableAuditEventFamily::RecoveryDecision,
        _ => DurableAuditEventFamily::GenericAudit,
    }
}

fn nonzero_u64(data: &[u8], start: usize) -> u64 {
    let mut bytes = [0u8; 8];
    for (index, target) in bytes.iter_mut().enumerate() {
        *target = byte_at(data, start + index);
    }
    u64::from_le_bytes(bytes).max(1)
}

fn byte_at(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or_default()
}

fn unique_journal_path() -> PathBuf {
    let case_id = CASE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "andromeda-durable-audit-fuzz-{}-{case_id}.journal",
        std::process::id()
    ))
}

fn cleanup_journal_files(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(anchor_path(path));
    let _ = fs::remove_file(sidecar_path(path, ".lock"));
    let _ = fs::remove_file(sidecar_path(path, ".compact.tmp"));
}

fn anchor_path(path: &Path) -> PathBuf {
    sidecar_path(path, ".chain")
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = OsString::from(path.as_os_str());
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}
