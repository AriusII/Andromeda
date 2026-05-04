use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    encode_wal_record, ConceptualRedoPlan, DatabaseManifest, Lsn, RecoveryPlan, StartupMode,
    WalRecord, WalRecordKind, WalScanResult, WalScanStop, WAL_BYTE_ORDER_LITTLE_ENDIAN,
    WAL_FORMAT_VERSION,
};

mod format;
mod report;
mod scan;

use format::{
    file_offset_for_wal_bytes, file_wal_header_checksum_without_checksum, write_file_wal_header,
    FILE_WAL_DATA_OFFSET,
};
use scan::{
    is_forensic_scan_stop, record_boundaries_for, scan_open_file_wal, FileWalRecordBoundary,
};

pub const FILE_WAL_MAGIC: u64 = 0x314c_4157_5244_4e41;
pub const FILE_WAL_HEADER_LEN: usize = 80;
pub const FILE_WAL_MONO_SEGMENT_ID: u64 = 1;

const FILE_WAL_HEADER_LEN_U32: u32 = FILE_WAL_HEADER_LEN as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalHeader {
    pub magic: u64,
    pub format_version: u16,
    pub byte_order: u16,
    pub header_length: u32,
    pub segment_id: u64,
    pub first_lsn: Lsn,
    pub base_previous_lsn: Option<Lsn>,
    pub durable_lsn: Lsn,
    pub durable_bytes: u64,
    pub durable_record_count: u64,
    pub header_checksum: u64,
    pub reserved: u64,
}

impl FileWalHeader {
    pub fn new(durable_lsn: Lsn, durable_bytes: u64, durable_record_count: u64) -> Self {
        let mut header = Self {
            magic: FILE_WAL_MAGIC,
            format_version: WAL_FORMAT_VERSION,
            byte_order: WAL_BYTE_ORDER_LITTLE_ENDIAN,
            header_length: FILE_WAL_HEADER_LEN_U32,
            segment_id: FILE_WAL_MONO_SEGMENT_ID,
            first_lsn: Lsn::new(1),
            base_previous_lsn: None,
            durable_lsn,
            durable_bytes,
            durable_record_count,
            header_checksum: 0,
            reserved: 0,
        };
        header.header_checksum = file_wal_header_checksum_without_checksum(&header);
        header
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != FILE_WAL_MAGIC {
            return Err(storage_error("file WAL magic mismatch"));
        }
        if self.format_version != WAL_FORMAT_VERSION {
            return Err(storage_error("unsupported file WAL format version"));
        }
        if self.byte_order != WAL_BYTE_ORDER_LITTLE_ENDIAN {
            return Err(storage_error("file WAL byte order mismatch"));
        }
        if self.header_length != FILE_WAL_HEADER_LEN_U32 {
            return Err(storage_error("file WAL header length mismatch"));
        }
        if self.segment_id != FILE_WAL_MONO_SEGMENT_ID {
            return Err(storage_error("file WAL segment id mismatch"));
        }
        if self.first_lsn != Lsn::new(1) {
            return Err(storage_error("file WAL first LSN must be 1"));
        }
        if self.base_previous_lsn.is_some() {
            return Err(storage_error(
                "file WAL mono-segment base LSN must be empty",
            ));
        }
        if self.reserved != 0 {
            return Err(storage_error("file WAL reserved bytes must be zero"));
        }
        if self.durable_lsn.is_zero() && (self.durable_bytes != 0 || self.durable_record_count != 0)
        {
            return Err(storage_error(
                "file WAL empty durable LSN must not carry durable bytes",
            ));
        }
        if !self.durable_lsn.is_zero() && self.durable_record_count == 0 {
            return Err(storage_error(
                "file WAL durable LSN requires at least one durable record",
            ));
        }
        if self.header_checksum != file_wal_header_checksum_without_checksum(self) {
            return Err(storage_error("file WAL header checksum mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWalDiskScan {
    pub header: FileWalHeader,
    pub physical_wal_bytes: u64,
    pub scanned_bytes: u64,
    pub durable_bytes: u64,
    pub durable_lsn: Lsn,
    pub scan: WalScanResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWalRecoveryBoundaryKind {
    Clean,
    RecoverableTail,
    ForensicChainBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWalRecoveryIgnoredTransactionReason {
    Incomplete,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalRecoveryReplayRecord {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<TransactionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalRecoveryIgnoredTransaction {
    pub transaction_id: TransactionId,
    pub reason: FileWalRecoveryIgnoredTransactionReason,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub record_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWalRecoveryReportV0 {
    pub startup_mode: StartupMode,
    pub header: FileWalHeader,
    pub physical_wal_bytes: u64,
    pub scanned_bytes: u64,
    pub durable_prefix_bytes: u64,
    pub durable_prefix_record_count: usize,
    pub durable_lsn: Lsn,
    pub scan_stop: Option<WalScanStop>,
    pub boundary_kind: FileWalRecoveryBoundaryKind,
    pub replay_records: Vec<FileWalRecoveryReplayRecord>,
    pub ignored_transactions: Vec<FileWalRecoveryIgnoredTransaction>,
    pub ignored_record_count: usize,
    pub forensic_required: bool,
}

impl FileWalRecoveryReportV0 {
    pub fn replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.replay_records.iter().map(|record| record.lsn)
    }

    pub fn ignored_transaction_ids(&self) -> impl Iterator<Item = TransactionId> + '_ {
        self.ignored_transactions
            .iter()
            .map(|transaction| transaction.transaction_id)
    }

    pub const fn has_recoverable_tail_boundary(&self) -> bool {
        matches!(
            self.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        )
    }
}

#[derive(Debug)]
pub struct FileWal {
    path: PathBuf,
    file: File,
    records: Vec<WalRecord>,
    record_boundaries: Vec<FileWalRecordBoundary>,
    append_bytes: u64,
    durable_bytes: u64,
    durable_lsn: Lsn,
    scan_stop: Option<WalScanStop>,
}

impl FileWal {
    pub fn open(path: impl AsRef<Path>) -> AndromedaResult<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|error| io_error("open file WAL", error))?;

        if file
            .metadata()
            .map_err(|error| io_error("read file WAL metadata", error))?
            .len()
            == 0
        {
            let header = FileWalHeader::new(Lsn::ZERO, 0, 0);
            write_file_wal_header(&mut file, &header)?;
            file.sync_all()
                .map_err(|error| io_error("sync new file WAL header", error))?;
        }

        let disk_scan = scan_open_file_wal(&mut file)?;
        if is_forensic_scan_stop(disk_scan.scan.stopped) {
            return Err(storage_error(
                "file WAL scan stopped at a non-recoverable LSN chain boundary",
            ));
        }

        let durable_bytes = u64::try_from(disk_scan.scan.valid_bytes)
            .map_err(|_| storage_error("file WAL durable byte count does not fit u64"))?;
        let durable_lsn = disk_scan.scan.last_valid_lsn.unwrap_or(Lsn::ZERO);
        let durable_record_count = disk_scan.scan.records.len() as u64;
        let header = FileWalHeader::new(durable_lsn, durable_bytes, durable_record_count);
        let desired_len = FILE_WAL_DATA_OFFSET
            .checked_add(durable_bytes)
            .ok_or_else(|| storage_error("file WAL length would overflow u64"))?;

        if file
            .metadata()
            .map_err(|error| io_error("read file WAL metadata", error))?
            .len()
            != desired_len
        {
            file.set_len(desired_len)
                .map_err(|error| io_error("truncate file WAL to durable prefix", error))?;
        }
        write_file_wal_header(&mut file, &header)?;
        file.sync_all()
            .map_err(|error| io_error("sync file WAL durable header", error))?;

        let record_boundaries = record_boundaries_for(&disk_scan.scan.records)?;

        Ok(Self {
            path,
            file,
            records: disk_scan.scan.records,
            record_boundaries,
            append_bytes: durable_bytes,
            durable_bytes,
            durable_lsn,
            scan_stop: disk_scan.scan.stopped,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn durable_lsn(&self) -> Lsn {
        self.durable_lsn
    }

    pub const fn durable_bytes(&self) -> u64 {
        self.durable_bytes
    }

    pub const fn append_bytes(&self) -> u64 {
        self.append_bytes
    }

    pub const fn scan_stop(&self) -> Option<WalScanStop> {
        self.scan_stop
    }

    pub fn records(&self) -> &[WalRecord] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn last_lsn(&self) -> Option<Lsn> {
        self.records.last().map(|record| record.header.lsn)
    }

    pub fn try_next_lsn(&self) -> AndromedaResult<Lsn> {
        match self.last_lsn() {
            Some(last_lsn) => last_lsn.try_next(),
            None => Ok(Lsn::new(1)),
        }
    }

    pub fn next_lsn(&self) -> Lsn {
        self.try_next_lsn().unwrap_or(Lsn::ZERO)
    }

    pub fn append(&mut self, record: WalRecord) -> AndromedaResult<Lsn> {
        record.validate()?;

        let expected_lsn = self.try_next_lsn()?;
        if record.header.lsn != expected_lsn {
            return Err(storage_error(
                "file WAL record must equal the next append LSN",
            ));
        }
        if record.header.previous_lsn != self.last_lsn() {
            return Err(storage_error(
                "file WAL record previous LSN must chain to the append tail",
            ));
        }

        let encoded = encode_wal_record(&record)?;
        self.file
            .seek(SeekFrom::Start(file_offset_for_wal_bytes(
                self.append_bytes,
            )?))
            .map_err(|error| io_error("seek file WAL append offset", error))?;
        self.file
            .write_all(&encoded)
            .map_err(|error| io_error("append file WAL record", error))?;

        self.append_bytes = self
            .append_bytes
            .checked_add(encoded.len() as u64)
            .ok_or_else(|| storage_error("file WAL append byte count would overflow u64"))?;
        self.record_boundaries.push(FileWalRecordBoundary {
            lsn: record.header.lsn,
            end_bytes: self.append_bytes,
        });
        let lsn = record.header.lsn;
        self.records.push(record);
        Ok(lsn)
    }

    pub fn append_payload(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Lsn> {
        let record = WalRecord::from_parts(
            kind,
            self.try_next_lsn()?,
            self.last_lsn(),
            transaction_id,
            payload,
        )?;
        self.append(record)
    }

    pub fn append_tx_begin(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxBegin, Some(transaction_id), Vec::new())
    }

    pub fn append_tx_commit(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxCommit, Some(transaction_id), Vec::new())
    }

    pub fn append_tx_rollback(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxRollback, Some(transaction_id), Vec::new())
    }

    pub fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
        if lsn.is_zero() || lsn <= self.durable_lsn {
            return Ok(self.durable_lsn);
        }

        let Some(last_lsn) = self.last_lsn() else {
            return Err(storage_error(
                "cannot flush file WAL before records are appended",
            ));
        };
        if lsn > last_lsn {
            return Err(storage_error(
                "cannot flush file WAL beyond the last appended LSN",
            ));
        }

        let durable_index = self
            .record_boundaries
            .iter()
            .position(|boundary| boundary.lsn == lsn)
            .ok_or_else(|| storage_error("file WAL flush LSN was not appended"))?;
        let durable_bytes = self.record_boundaries[durable_index].end_bytes;
        let durable_record_count = (durable_index + 1) as u64;

        self.file
            .sync_data()
            .map_err(|error| io_error("sync file WAL records", error))?;

        self.durable_lsn = lsn;
        self.durable_bytes = durable_bytes;
        let header = FileWalHeader::new(self.durable_lsn, durable_bytes, durable_record_count);
        write_file_wal_header(&mut self.file, &header)?;
        self.file
            .sync_all()
            .map_err(|error| io_error("sync file WAL header", error))?;

        Ok(self.durable_lsn)
    }

    pub fn flush_all(&mut self) -> AndromedaResult<Lsn> {
        match self.last_lsn() {
            Some(last_lsn) => self.flush_through(last_lsn),
            None => Ok(self.durable_lsn),
        }
    }

    pub fn durable_records(&self) -> impl Iterator<Item = &WalRecord> {
        let durable_lsn = self.durable_lsn;
        self.records
            .iter()
            .filter(move |record| record.header.lsn <= durable_lsn)
    }

    pub fn replay_durable(&self) -> Vec<WalRecord> {
        self.durable_records().cloned().collect()
    }

    pub fn scan_path(path: impl AsRef<Path>) -> AndromedaResult<FileWalDiskScan> {
        scan_file_wal(path)
    }
}

pub fn scan_file_wal(path: impl AsRef<Path>) -> AndromedaResult<FileWalDiskScan> {
    scan::scan_file_wal(path)
}

pub fn recover_from_file_wal(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<ConceptualRedoPlan> {
    let disk_scan = scan_file_wal(path)?;
    RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)
}

pub fn report_file_wal_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    report::report_file_wal_recovery_v0(manifest, startup_mode, path)
}

fn io_error(action: &str, error: std::io::Error) -> AndromedaError {
    storage_error(format!("{action}: {error}"))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RedoRecordDecision, WalScanStopReason};
    use std::fs::{metadata, remove_file, OpenOptions};

    fn test_wal_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "andromeda-storage-{test_name}-{}.wal",
            std::process::id()
        ))
    }

    fn recovery_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        }
    }

    #[test]
    fn file_wal_open_truncates_unflushed_physical_tail_before_replay() {
        let path = test_wal_path("truncates-unflushed-tail");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(101);
        let unflushed_tx = TransactionId::new(102);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            let committed_row_lsn = wal
                .append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.flush_all().unwrap();

            wal.append_tx_begin(unflushed_tx).unwrap();
            wal.append_payload(WalRecordKind::RowUpdate, Some(unflushed_tx), b"not-durable")
                .unwrap();
            assert_eq!(committed_row_lsn, Lsn::new(2));
            assert_eq!(wal.last_lsn(), Some(Lsn::new(5)));
            assert_eq!(wal.durable_lsn(), Lsn::new(3));
        }

        let wal = FileWal::open(&path).unwrap();
        let expected_len = FILE_WAL_DATA_OFFSET + wal.durable_bytes();
        assert_eq!(wal.last_lsn(), Some(Lsn::new(3)));
        assert_eq!(wal.durable_lsn(), Lsn::new(3));
        assert_eq!(metadata(&path).unwrap().len(), expected_len);
        drop(wal);

        let plan =
            recover_from_file_wal(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert!(!plan
            .incomplete_transactions
            .iter()
            .any(|transaction| transaction.transaction_id == unflushed_tx));

        remove_file(&path).ok();
    }

    #[test]
    fn file_wal_recovery_skips_transaction_with_truncated_durable_commit() {
        let path = test_wal_path("truncated-durable-commit");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(103);
        let tail_tx = TransactionId::new(104);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            wal.append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.append_tx_begin(tail_tx).unwrap();
            wal.append_payload(WalRecordKind::RowUpdate, Some(tail_tx), b"tail")
                .unwrap();
            wal.append_tx_commit(tail_tx).unwrap();
            wal.flush_all().unwrap();
            assert_eq!(wal.durable_lsn(), Lsn::new(6));
        }
        let original_len = metadata(&path).unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(original_len - 1)
            .unwrap();

        let report =
            report_file_wal_recovery_v0(&recovery_manifest(), StartupMode::SafeStart, &path)
                .unwrap();

        assert_eq!(
            report.scan_stop.unwrap().reason,
            WalScanStopReason::TruncatedHeader
        );
        assert_eq!(
            report.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        );
        assert_eq!(report.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(report.ignored_transactions.len(), 1);
        assert_eq!(report.ignored_transactions[0].transaction_id, tail_tx);
        assert_eq!(
            report.ignored_transactions[0].reason,
            FileWalRecoveryIgnoredTransactionReason::Incomplete
        );
        assert_eq!(report.ignored_record_count, 1);

        let plan =
            recover_from_file_wal(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(plan.incomplete_transactions.len(), 1);
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == Lsn::new(5))
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );

        remove_file(&path).ok();
    }
}
