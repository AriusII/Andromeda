use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    Lsn, WalRecord, WalRecordKind, WalScanStop, encode_wal_record, write_ahead_log::append_chain,
};

use super::{
    FileWalHeader,
    format::{file_offset_for_wal_bytes, write_file_wal_header},
    scan::{
        FileWalDiskScan, FileWalRecordBoundary, is_forensic_scan_stop, record_boundaries_for,
        scan_file_wal, scan_open_file_wal,
    },
    storage_error,
};

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
            .truncate(false)
            .open(&path)
            .map_err(|error| super::io_error("open file WAL", error))?;

        if file
            .metadata()
            .map_err(|error| super::io_error("read file WAL metadata", error))?
            .len()
            == 0
        {
            let header = FileWalHeader::new(Lsn::ZERO, 0, 0);
            write_file_wal_header(&mut file, &header)?;
            file.sync_all()
                .map_err(|error| super::io_error("sync new file WAL header", error))?;
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
        let desired_len = file_offset_for_wal_bytes(durable_bytes)?;

        if file
            .metadata()
            .map_err(|error| super::io_error("read file WAL metadata", error))?
            .len()
            != desired_len
        {
            file.set_len(desired_len)
                .map_err(|error| super::io_error("truncate file WAL to durable prefix", error))?;
        }
        write_file_wal_header(&mut file, &header)?;
        file.sync_all()
            .map_err(|error| super::io_error("sync file WAL durable header", error))?;

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
        append_chain::try_next_lsn(self.last_lsn())
    }

    pub fn next_lsn(&self) -> Lsn {
        append_chain::next_lsn_or_zero(self.last_lsn())
    }

    pub fn append(&mut self, record: WalRecord) -> AndromedaResult<Lsn> {
        append_chain::validate_append_record(
            &record,
            self.last_lsn(),
            "file WAL record must equal the next append LSN",
            "file WAL record previous LSN must chain to the append tail",
        )?;

        let encoded = encode_wal_record(&record)?;
        self.file
            .seek(SeekFrom::Start(file_offset_for_wal_bytes(
                self.append_bytes,
            )?))
            .map_err(|error| super::io_error("seek file WAL append offset", error))?;
        self.file
            .write_all(&encoded)
            .map_err(|error| super::io_error("append file WAL record", error))?;

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
        let record = append_chain::payload_record(kind, self.last_lsn(), transaction_id, payload)?;
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
        let Some(lsn) = append_chain::validate_flush_target(
            lsn,
            self.durable_lsn,
            self.last_lsn(),
            "cannot flush file WAL before records are appended",
            "cannot flush file WAL beyond the last appended LSN",
        )?
        else {
            return Ok(self.durable_lsn);
        };

        let durable_index = self
            .record_boundaries
            .iter()
            .position(|boundary| boundary.lsn == lsn)
            .ok_or_else(|| storage_error("file WAL flush LSN was not appended"))?;
        let durable_bytes = self.record_boundaries[durable_index].end_bytes;
        let durable_record_count = (durable_index + 1) as u64;

        self.file
            .sync_data()
            .map_err(|error| super::io_error("sync file WAL records", error))?;

        self.durable_lsn = lsn;
        self.durable_bytes = durable_bytes;
        let header = FileWalHeader::new(self.durable_lsn, durable_bytes, durable_record_count);
        write_file_wal_header(&mut self.file, &header)?;
        self.file
            .sync_all()
            .map_err(|error| super::io_error("sync file WAL header", error))?;

        Ok(self.durable_lsn)
    }

    pub fn flush_all(&mut self) -> AndromedaResult<Lsn> {
        match self.last_lsn() {
            Some(last_lsn) => self.flush_through(last_lsn),
            None => Ok(self.durable_lsn),
        }
    }

    pub fn durable_records(&self) -> impl Iterator<Item = &WalRecord> {
        append_chain::durable_records(&self.records, self.durable_lsn)
    }

    pub fn replay_durable(&self) -> Vec<WalRecord> {
        append_chain::replay_durable(&self.records, self.durable_lsn)
    }

    pub fn scan_path(path: impl AsRef<Path>) -> AndromedaResult<FileWalDiskScan> {
        scan_file_wal(path)
    }
}
