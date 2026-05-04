use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    encode_wal_record, scan_wal_records, summarize_transactions_from_records, ConceptualRedoPlan,
    DatabaseManifest, DurableTransactionState, Lsn, RecoveryPlan, RedoRecordDecision,
    RedoRecordPlan, StartupMode, WalRecord, WalRecordKind, WalScanResult, WalScanStop,
    WalScanStopReason, WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION,
};

pub const FILE_WAL_MAGIC: u64 = 0x314c_4157_5244_4e41;
pub const FILE_WAL_HEADER_LEN: usize = 80;
pub const FILE_WAL_MONO_SEGMENT_ID: u64 = 1;

const FILE_WAL_HEADER_LEN_U32: u32 = FILE_WAL_HEADER_LEN as u32;
const FILE_WAL_DATA_OFFSET: u64 = FILE_WAL_HEADER_LEN as u64;

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
    let mut file = File::open(path.as_ref()).map_err(|error| io_error("open file WAL", error))?;
    scan_open_file_wal(&mut file)
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
    manifest.validate()?;
    let disk_scan = scan_file_wal(path)?;
    let boundary_kind = recovery_boundary_kind(disk_scan.scan.stopped);
    let forensic_required = matches!(
        boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );

    let (replay_records, ignored_transactions, ignored_record_count) = if forensic_required {
        (
            Vec::new(),
            ignored_transactions_from_prefix(&disk_scan.scan.records),
            0,
        )
    } else {
        let plan =
            RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)?;
        (
            recovery_report_replay_records(&plan),
            recovery_report_ignored_transactions(&plan),
            recovery_report_ignored_record_count(&plan),
        )
    };

    Ok(FileWalRecoveryReportV0 {
        startup_mode,
        header: disk_scan.header,
        physical_wal_bytes: disk_scan.physical_wal_bytes,
        scanned_bytes: disk_scan.scanned_bytes,
        durable_prefix_bytes: disk_scan.durable_bytes,
        durable_prefix_record_count: disk_scan.scan.records.len(),
        durable_lsn: disk_scan.durable_lsn,
        scan_stop: disk_scan.scan.stopped,
        boundary_kind,
        replay_records,
        ignored_transactions,
        ignored_record_count,
        forensic_required,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileWalRecordBoundary {
    lsn: Lsn,
    end_bytes: u64,
}

fn scan_open_file_wal(file: &mut File) -> AndromedaResult<FileWalDiskScan> {
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

fn read_file_wal_header(file: &mut File) -> AndromedaResult<FileWalHeader> {
    let mut bytes = [0; FILE_WAL_HEADER_LEN];
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek file WAL header", error))?;
    file.read_exact(&mut bytes)
        .map_err(|error| io_error("read file WAL header", error))?;
    decode_file_wal_header(&bytes)
}

fn write_file_wal_header(file: &mut File, header: &FileWalHeader) -> AndromedaResult<()> {
    header.validate()?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek file WAL header", error))?;
    file.write_all(&encode_file_wal_header(header))
        .map_err(|error| io_error("write file WAL header", error))
}

fn encode_file_wal_header(header: &FileWalHeader) -> [u8; FILE_WAL_HEADER_LEN] {
    let mut bytes = [0; FILE_WAL_HEADER_LEN];
    write_u64(&mut bytes, 0, header.magic);
    write_u16(&mut bytes, 8, header.format_version);
    write_u16(&mut bytes, 10, header.byte_order);
    write_u32(&mut bytes, 12, header.header_length);
    write_u64(&mut bytes, 16, header.segment_id);
    write_u64(&mut bytes, 24, header.first_lsn.get());
    write_u64(&mut bytes, 32, header.base_previous_lsn.map_or(0, Lsn::get));
    write_u64(&mut bytes, 40, header.durable_lsn.get());
    write_u64(&mut bytes, 48, header.durable_bytes);
    write_u64(&mut bytes, 56, header.durable_record_count);
    write_u64(&mut bytes, 64, header.header_checksum);
    write_u64(&mut bytes, 72, header.reserved);
    bytes
}

fn decode_file_wal_header(bytes: &[u8; FILE_WAL_HEADER_LEN]) -> AndromedaResult<FileWalHeader> {
    let base_previous_lsn = read_u64(bytes, 32);
    let header = FileWalHeader {
        magic: read_u64(bytes, 0),
        format_version: read_u16(bytes, 8),
        byte_order: read_u16(bytes, 10),
        header_length: read_u32(bytes, 12),
        segment_id: read_u64(bytes, 16),
        first_lsn: Lsn::new(read_u64(bytes, 24)),
        base_previous_lsn: (base_previous_lsn != 0).then_some(Lsn::new(base_previous_lsn)),
        durable_lsn: Lsn::new(read_u64(bytes, 40)),
        durable_bytes: read_u64(bytes, 48),
        durable_record_count: read_u64(bytes, 56),
        header_checksum: read_u64(bytes, 64),
        reserved: read_u64(bytes, 72),
    };
    header.validate()?;
    Ok(header)
}

fn file_wal_header_checksum_without_checksum(header: &FileWalHeader) -> u64 {
    let mut bytes = Vec::with_capacity(FILE_WAL_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.byte_order);
    push_u32(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.segment_id);
    push_u64(&mut bytes, header.first_lsn.get());
    push_u64(&mut bytes, header.base_previous_lsn.map_or(0, Lsn::get));
    push_u64(&mut bytes, header.durable_lsn.get());
    push_u64(&mut bytes, header.durable_bytes);
    push_u64(&mut bytes, header.durable_record_count);
    push_u64(&mut bytes, header.reserved);
    fnv64_nonzero(&bytes)
}

fn record_boundaries_for(records: &[WalRecord]) -> AndromedaResult<Vec<FileWalRecordBoundary>> {
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

fn file_offset_for_wal_bytes(wal_bytes: u64) -> AndromedaResult<u64> {
    FILE_WAL_DATA_OFFSET
        .checked_add(wal_bytes)
        .ok_or_else(|| storage_error("file WAL offset would overflow u64"))
}

fn is_forensic_scan_stop(stop: Option<WalScanStop>) -> bool {
    matches!(
        stop.map(|stop| stop.reason),
        Some(
            WalScanStopReason::LsnGap
                | WalScanStopReason::DuplicateOrReorderedLsn
                | WalScanStopReason::PreviousLsnMismatch
        )
    )
}

fn recovery_boundary_kind(stop: Option<WalScanStop>) -> FileWalRecoveryBoundaryKind {
    if is_forensic_scan_stop(stop) {
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    } else if stop.is_some() {
        FileWalRecoveryBoundaryKind::RecoverableTail
    } else {
        FileWalRecoveryBoundaryKind::Clean
    }
}

fn recovery_report_replay_records(plan: &ConceptualRedoPlan) -> Vec<FileWalRecoveryReplayRecord> {
    plan.records
        .iter()
        .filter(|record| record.should_replay())
        .map(report_replay_record_from_redo_record)
        .collect()
}

fn report_replay_record_from_redo_record(record: &RedoRecordPlan) -> FileWalRecoveryReplayRecord {
    FileWalRecoveryReplayRecord {
        lsn: record.lsn,
        kind: record.kind,
        transaction_id: record.transaction_id,
    }
}

fn recovery_report_ignored_transactions(
    plan: &ConceptualRedoPlan,
) -> Vec<FileWalRecoveryIgnoredTransaction> {
    plan.transaction_evidence
        .iter()
        .filter_map(|summary| match summary.state {
            DurableTransactionState::RolledBack => Some(FileWalRecoveryIgnoredTransaction {
                transaction_id: summary.transaction_id,
                reason: FileWalRecoveryIgnoredTransactionReason::RolledBack,
                first_lsn: summary.first_lsn,
                last_lsn: summary.last_lsn,
                record_count: summary.record_count,
            }),
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                Some(FileWalRecoveryIgnoredTransaction {
                    transaction_id: summary.transaction_id,
                    reason: FileWalRecoveryIgnoredTransactionReason::Incomplete,
                    first_lsn: summary.first_lsn,
                    last_lsn: summary.last_lsn,
                    record_count: summary.record_count,
                })
            }
            DurableTransactionState::Committed => None,
        })
        .collect()
}

fn recovery_report_ignored_record_count(plan: &ConceptualRedoPlan) -> usize {
    plan.records
        .iter()
        .filter(|record| {
            matches!(
                record.decision,
                RedoRecordDecision::SkipIncompleteTransaction
                    | RedoRecordDecision::SkipRolledBackTransaction
            )
        })
        .count()
}

fn ignored_transactions_from_prefix(
    records: &[WalRecord],
) -> Vec<FileWalRecoveryIgnoredTransaction> {
    summarize_transactions_from_records(records)
        .into_iter()
        .filter_map(|summary| match summary.state {
            DurableTransactionState::RolledBack => Some(FileWalRecoveryIgnoredTransaction {
                transaction_id: summary.transaction_id,
                reason: FileWalRecoveryIgnoredTransactionReason::RolledBack,
                first_lsn: summary.first_lsn,
                last_lsn: summary.last_lsn,
                record_count: summary.record_count,
            }),
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                Some(FileWalRecoveryIgnoredTransaction {
                    transaction_id: summary.transaction_id,
                    reason: FileWalRecoveryIgnoredTransactionReason::Incomplete,
                    first_lsn: summary.first_lsn,
                    last_lsn: summary.last_lsn,
                    record_count: summary.record_count,
                })
            }
            DurableTransactionState::Committed => None,
        })
        .collect()
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("file WAL u16 slice"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("file WAL u32 slice"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("file WAL u64 slice"),
    )
}

fn fnv64_nonzero(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 {
        1
    } else {
        state
    }
}

fn io_error(action: &str, error: std::io::Error) -> AndromedaError {
    storage_error(format!("{action}: {error}"))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
