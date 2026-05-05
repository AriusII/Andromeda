use andromeda_core::AndromedaResult;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::{Lsn, WalRecord, WalScanStop, WalScanStopReason, encode_wal_record, scan_wal_records};

use super::{
    FileWalDiskScan,
    format::{FILE_WAL_DATA_OFFSET, read_file_wal_header},
    io_error, storage_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileWalRecordBoundary {
    pub(super) lsn: Lsn,
    pub(super) end_bytes: u64,
}

pub fn scan_file_wal(path: impl AsRef<Path>) -> AndromedaResult<FileWalDiskScan> {
    let mut file = File::open(path.as_ref()).map_err(|error| io_error("open file WAL", error))?;
    scan_open_file_wal(&mut file)
}

pub(super) fn scan_open_file_wal(file: &mut File) -> AndromedaResult<FileWalDiskScan> {
    let physical_len = file
        .metadata()
        .map_err(|error| io_error("read file WAL metadata", error))?
        .len();
    if physical_len < FILE_WAL_DATA_OFFSET {
        return Err(storage_error("file WAL header is truncated"));
    }

    let header = read_file_wal_header(file)?;
    header.validate()?;

    let physical_wal_bytes = physical_len - FILE_WAL_DATA_OFFSET;
    let scanned_bytes = header.durable_bytes.min(physical_wal_bytes);
    let scanned_len = usize::try_from(scanned_bytes)
        .map_err(|_| storage_error("file WAL scan byte count does not fit usize"))?;
    let mut bytes = vec![0; scanned_len];
    file.seek(SeekFrom::Start(FILE_WAL_DATA_OFFSET))
        .map_err(|error| io_error("seek file WAL data", error))?;
    file.read_exact(&mut bytes)
        .map_err(|error| io_error("read file WAL durable bytes", error))?;

    let scan = scan_wal_records(&bytes);
    let durable_bytes = u64::try_from(scan.valid_bytes)
        .map_err(|_| storage_error("file WAL valid byte count does not fit u64"))?;
    let durable_lsn = scan.last_valid_lsn.unwrap_or(Lsn::ZERO);

    Ok(FileWalDiskScan {
        header,
        physical_wal_bytes,
        scanned_bytes,
        durable_bytes,
        durable_lsn,
        scan,
    })
}

pub(super) fn record_boundaries_for(
    records: &[WalRecord],
) -> AndromedaResult<Vec<FileWalRecordBoundary>> {
    let mut offset = 0u64;
    let mut boundaries = Vec::with_capacity(records.len());
    for record in records {
        let encoded_len = encode_wal_record(record)?.len() as u64;
        offset = offset
            .checked_add(encoded_len)
            .ok_or_else(|| storage_error("file WAL record boundary would overflow u64"))?;
        boundaries.push(FileWalRecordBoundary {
            lsn: record.header.lsn,
            end_bytes: offset,
        });
    }
    Ok(boundaries)
}

pub(super) fn is_forensic_scan_stop(stop: Option<WalScanStop>) -> bool {
    matches!(
        stop.map(|stop| stop.reason),
        Some(
            WalScanStopReason::LsnGap
                | WalScanStopReason::DuplicateOrReorderedLsn
                | WalScanStopReason::PreviousLsnMismatch
        )
    )
}
