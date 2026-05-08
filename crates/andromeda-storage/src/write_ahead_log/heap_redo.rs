//! Versioned heap row redo WAL payloads.
//!
//! `HREDOV1` is the production-side payload envelope consumed by recovery for
//! `RowInsert`, `RowDelete`, and `RowUpdate` records. The codec is deliberately
//! narrow: it only describes row redo intent and does not mutate heap pages.

use std::{error::Error, fmt};

use crate::{Lsn, PageId, PageSize, WalRecordKind};

pub const HEAP_ROW_REDO_PAYLOAD_MAGIC: &[u8; 8] = b"HREDOV1\0";
pub const HEAP_ROW_REDO_PAYLOAD_VERSION: u16 = 1;
pub const HEAP_ROW_REDO_HEADER_LEN: usize = 44;
pub const HEAP_ROW_REDO_NONE_SLOT_ID: u16 = u16::MAX;

const HEAP_ROW_REDO_MAGIC_OFFSET: usize = 0;
const HEAP_ROW_REDO_VERSION_OFFSET: usize = 8;
const HEAP_ROW_REDO_OPERATION_OFFSET: usize = 10;
const HEAP_ROW_REDO_PAGE_SIZE_OFFSET: usize = 11;
const HEAP_ROW_REDO_PAGE_ID_OFFSET: usize = 12;
const HEAP_ROW_REDO_BEFORE_SLOT_OFFSET: usize = 20;
const HEAP_ROW_REDO_AFTER_SLOT_OFFSET: usize = 22;
const HEAP_ROW_REDO_EXPECTED_PREVIOUS_LSN_OFFSET: usize = 24;
const HEAP_ROW_REDO_RESULTING_LSN_OFFSET: usize = 32;
const HEAP_ROW_REDO_TUPLE_LEN_OFFSET: usize = 40;

const OP_ROW_INSERT: u8 = 1;
const OP_ROW_DELETE: u8 = 2;
const OP_ROW_UPDATE: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapRowRedoPayloadError {
    message: String,
}

impl HeapRowRedoPayloadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HeapRowRedoPayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for HeapRowRedoPayloadError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapRowRedoOperation {
    Insert,
    Delete,
    Update,
}

impl HeapRowRedoOperation {
    pub const fn wal_record_kind(self) -> WalRecordKind {
        match self {
            Self::Insert => WalRecordKind::RowInsert,
            Self::Delete => WalRecordKind::RowDelete,
            Self::Update => WalRecordKind::RowUpdate,
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::Insert => OP_ROW_INSERT,
            Self::Delete => OP_ROW_DELETE,
            Self::Update => OP_ROW_UPDATE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapRowRedoPayloadV1 {
    pub(crate) operation: HeapRowRedoOperation,
    pub(crate) page_id: PageId,
    pub(crate) page_size: PageSize,
    pub(crate) before_slot_id: u16,
    pub(crate) after_slot_id: u16,
    pub(crate) expected_previous_page_lsn: Lsn,
    pub(crate) resulting_page_lsn: Lsn,
    pub(crate) tuple: Vec<u8>,
}

impl HeapRowRedoPayloadV1 {
    pub fn row_insert(
        page_id: PageId,
        page_size: PageSize,
        slot_id: u16,
        expected_previous_page_lsn: Lsn,
        resulting_page_lsn: Lsn,
        tuple: impl Into<Vec<u8>>,
    ) -> Result<Self, HeapRowRedoPayloadError> {
        Self::build(HeapRowRedoPayloadParts {
            operation: HeapRowRedoOperation::Insert,
            page_id,
            page_size,
            before_slot_id: HEAP_ROW_REDO_NONE_SLOT_ID,
            after_slot_id: slot_id,
            expected_previous_page_lsn,
            resulting_page_lsn,
            tuple: tuple.into(),
        })
    }

    pub fn row_delete(
        page_id: PageId,
        page_size: PageSize,
        slot_id: u16,
        expected_previous_page_lsn: Lsn,
        resulting_page_lsn: Lsn,
    ) -> Result<Self, HeapRowRedoPayloadError> {
        Self::build(HeapRowRedoPayloadParts {
            operation: HeapRowRedoOperation::Delete,
            page_id,
            page_size,
            before_slot_id: slot_id,
            after_slot_id: HEAP_ROW_REDO_NONE_SLOT_ID,
            expected_previous_page_lsn,
            resulting_page_lsn,
            tuple: Vec::new(),
        })
    }

    pub fn row_update(
        page_id: PageId,
        page_size: PageSize,
        before_slot_id: u16,
        after_slot_id: u16,
        expected_previous_page_lsn: Lsn,
        resulting_page_lsn: Lsn,
        new_tuple: impl Into<Vec<u8>>,
    ) -> Result<Self, HeapRowRedoPayloadError> {
        Self::build(HeapRowRedoPayloadParts {
            operation: HeapRowRedoOperation::Update,
            page_id,
            page_size,
            before_slot_id,
            after_slot_id,
            expected_previous_page_lsn,
            resulting_page_lsn,
            tuple: new_tuple.into(),
        })
    }

    pub fn decode(bytes: &[u8], kind: WalRecordKind) -> Result<Self, HeapRowRedoPayloadError> {
        if bytes.len() < HEAP_ROW_REDO_HEADER_LEN {
            return Err(error(format!(
                "expected at least {HEAP_ROW_REDO_HEADER_LEN} bytes, got {}",
                bytes.len()
            )));
        }
        if read_bytes(
            bytes,
            HEAP_ROW_REDO_MAGIC_OFFSET,
            HEAP_ROW_REDO_PAYLOAD_MAGIC.len(),
        )? != HEAP_ROW_REDO_PAYLOAD_MAGIC.as_slice()
        {
            return Err(error("missing HREDOV1 heap row redo envelope magic"));
        }

        let version = read_u16(bytes, HEAP_ROW_REDO_VERSION_OFFSET)?;
        if version != HEAP_ROW_REDO_PAYLOAD_VERSION {
            return Err(error(format!(
                "unsupported heap row redo payload version {version}"
            )));
        }

        let operation_tag = read_u8(bytes, HEAP_ROW_REDO_OPERATION_OFFSET)?;
        let expected_tag = operation_tag_for_kind(kind).ok_or_else(|| {
            error(format!(
                "heap row redo payload cannot decode WAL record kind {kind:?}"
            ))
        })?;
        if operation_tag != expected_tag {
            return Err(error(format!(
                "operation tag {operation_tag} does not match record kind {kind:?}"
            )));
        }

        let operation = operation_from_tag(operation_tag).ok_or_else(|| {
            error(format!(
                "unknown heap row redo operation tag {operation_tag}"
            ))
        })?;
        let page_size = page_size_from_tag(read_u8(bytes, HEAP_ROW_REDO_PAGE_SIZE_OFFSET)?)?;
        let page_id = PageId::new(read_u64(bytes, HEAP_ROW_REDO_PAGE_ID_OFFSET)?);
        if page_id.is_zero() {
            return Err(error("page_id must not be zero"));
        }

        let before_slot_id = read_u16(bytes, HEAP_ROW_REDO_BEFORE_SLOT_OFFSET)?;
        let after_slot_id = read_u16(bytes, HEAP_ROW_REDO_AFTER_SLOT_OFFSET)?;
        let expected_previous_page_lsn =
            Lsn::new(read_u64(bytes, HEAP_ROW_REDO_EXPECTED_PREVIOUS_LSN_OFFSET)?);
        let resulting_page_lsn = Lsn::new(read_u64(bytes, HEAP_ROW_REDO_RESULTING_LSN_OFFSET)?);
        validate_lsn_pair(expected_previous_page_lsn, resulting_page_lsn)?;

        let tuple_len = usize::try_from(read_u32(bytes, HEAP_ROW_REDO_TUPLE_LEN_OFFSET)?)
            .map_err(|_| error("heap row redo tuple length does not fit usize"))?;
        let expected_len = HEAP_ROW_REDO_HEADER_LEN
            .checked_add(tuple_len)
            .ok_or_else(|| error("heap row redo tuple length overflows payload size"))?;
        if bytes.len() != expected_len {
            return Err(error(format!(
                "tuple length {tuple_len} does not match payload length {}; expected {expected_len}",
                bytes.len()
            )));
        }
        let tuple = read_bytes(bytes, HEAP_ROW_REDO_HEADER_LEN, tuple_len)?.to_vec();

        validate_slots(operation, before_slot_id, after_slot_id, &tuple)?;

        Ok(Self {
            operation,
            page_id,
            page_size,
            before_slot_id,
            after_slot_id,
            expected_previous_page_lsn,
            resulting_page_lsn,
            tuple,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(HEAP_ROW_REDO_HEADER_LEN + self.tuple.len());
        payload.extend_from_slice(HEAP_ROW_REDO_PAYLOAD_MAGIC);
        payload.extend_from_slice(&HEAP_ROW_REDO_PAYLOAD_VERSION.to_le_bytes());
        payload.push(self.operation.tag());
        payload.push(page_size_tag(self.page_size));
        payload.extend_from_slice(&self.page_id.get().to_le_bytes());
        payload.extend_from_slice(&self.before_slot_id.to_le_bytes());
        payload.extend_from_slice(&self.after_slot_id.to_le_bytes());
        payload.extend_from_slice(&self.expected_previous_page_lsn.get().to_le_bytes());
        payload.extend_from_slice(&self.resulting_page_lsn.get().to_le_bytes());
        payload.extend_from_slice(&(self.tuple.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.tuple);
        payload
    }

    pub fn encode_for_wal_kind(
        &self,
        kind: WalRecordKind,
    ) -> Result<Vec<u8>, HeapRowRedoPayloadError> {
        if self.operation.wal_record_kind() != kind {
            return Err(error(format!(
                "heap row redo operation {:?} cannot be encoded for WAL record kind {kind:?}",
                self.operation
            )));
        }
        Ok(self.encode())
    }

    pub const fn operation(&self) -> HeapRowRedoOperation {
        self.operation
    }

    pub const fn wal_record_kind(&self) -> WalRecordKind {
        self.operation.wal_record_kind()
    }

    pub const fn page_id(&self) -> PageId {
        self.page_id
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub const fn before_slot_id(&self) -> u16 {
        self.before_slot_id
    }

    pub const fn after_slot_id(&self) -> u16 {
        self.after_slot_id
    }

    pub const fn expected_previous_page_lsn(&self) -> Lsn {
        self.expected_previous_page_lsn
    }

    pub const fn resulting_page_lsn(&self) -> Lsn {
        self.resulting_page_lsn
    }

    pub fn tuple(&self) -> &[u8] {
        &self.tuple
    }

    fn build(parts: HeapRowRedoPayloadParts) -> Result<Self, HeapRowRedoPayloadError> {
        if parts.page_id.is_zero() {
            return Err(error("page_id must not be zero"));
        }
        validate_lsn_pair(parts.expected_previous_page_lsn, parts.resulting_page_lsn)?;
        validate_slots(
            parts.operation,
            parts.before_slot_id,
            parts.after_slot_id,
            &parts.tuple,
        )?;
        if parts.tuple.len() > u32::MAX as usize {
            return Err(error("heap row redo tuple length exceeds u32"));
        }

        Ok(Self {
            operation: parts.operation,
            page_id: parts.page_id,
            page_size: parts.page_size,
            before_slot_id: parts.before_slot_id,
            after_slot_id: parts.after_slot_id,
            expected_previous_page_lsn: parts.expected_previous_page_lsn,
            resulting_page_lsn: parts.resulting_page_lsn,
            tuple: parts.tuple,
        })
    }
}

struct HeapRowRedoPayloadParts {
    operation: HeapRowRedoOperation,
    page_id: PageId,
    page_size: PageSize,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    resulting_page_lsn: Lsn,
    tuple: Vec<u8>,
}

fn validate_lsn_pair(
    expected_previous_page_lsn: Lsn,
    resulting_page_lsn: Lsn,
) -> Result<(), HeapRowRedoPayloadError> {
    if resulting_page_lsn.is_zero() {
        return Err(error("heap row redo resulting page LSN must not be zero"));
    }
    if !expected_previous_page_lsn.is_zero() && expected_previous_page_lsn >= resulting_page_lsn {
        return Err(error(
            "expected_previous_page_lsn must be zero or precede resulting_page_lsn",
        ));
    }
    Ok(())
}

fn validate_slots(
    operation: HeapRowRedoOperation,
    before_slot_id: u16,
    after_slot_id: u16,
    tuple: &[u8],
) -> Result<(), HeapRowRedoPayloadError> {
    match operation {
        HeapRowRedoOperation::Insert => {
            if before_slot_id != HEAP_ROW_REDO_NONE_SLOT_ID {
                return Err(error("RowInsert before_slot_id must be NONE_SLOT_ID"));
            }
            if after_slot_id == HEAP_ROW_REDO_NONE_SLOT_ID {
                return Err(error(
                    "RowInsert after_slot_id must identify a concrete slot",
                ));
            }
            if tuple.is_empty() {
                return Err(error("RowInsert tuple bytes must not be empty"));
            }
        },
        HeapRowRedoOperation::Delete => {
            if before_slot_id == HEAP_ROW_REDO_NONE_SLOT_ID {
                return Err(error(
                    "RowDelete before_slot_id must identify a concrete slot",
                ));
            }
            if after_slot_id != HEAP_ROW_REDO_NONE_SLOT_ID {
                return Err(error("RowDelete after_slot_id must be NONE_SLOT_ID"));
            }
            if !tuple.is_empty() {
                return Err(error("RowDelete tuple bytes must be empty"));
            }
        },
        HeapRowRedoOperation::Update => {
            if before_slot_id == HEAP_ROW_REDO_NONE_SLOT_ID
                || after_slot_id == HEAP_ROW_REDO_NONE_SLOT_ID
            {
                return Err(error(
                    "RowUpdate before_slot_id and after_slot_id must identify concrete slots",
                ));
            }
            if before_slot_id == after_slot_id {
                return Err(error(
                    "RowUpdate must close an old slot and insert a new slot",
                ));
            }
            if tuple.is_empty() {
                return Err(error("RowUpdate new tuple bytes must not be empty"));
            }
        },
    }
    Ok(())
}

const fn operation_tag_for_kind(kind: WalRecordKind) -> Option<u8> {
    match kind {
        WalRecordKind::RowInsert => Some(OP_ROW_INSERT),
        WalRecordKind::RowDelete => Some(OP_ROW_DELETE),
        WalRecordKind::RowUpdate => Some(OP_ROW_UPDATE),
        _ => None,
    }
}

const fn operation_from_tag(tag: u8) -> Option<HeapRowRedoOperation> {
    match tag {
        OP_ROW_INSERT => Some(HeapRowRedoOperation::Insert),
        OP_ROW_DELETE => Some(HeapRowRedoOperation::Delete),
        OP_ROW_UPDATE => Some(HeapRowRedoOperation::Update),
        _ => None,
    }
}

const fn page_size_tag(page_size: PageSize) -> u8 {
    match page_size {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    }
}

fn page_size_from_tag(tag: u8) -> Result<PageSize, HeapRowRedoPayloadError> {
    match tag {
        1 => Ok(PageSize::KiB16),
        2 => Ok(PageSize::KiB32),
        _ => Err(error(format!(
            "unsupported heap row redo page size tag {tag}"
        ))),
    }
}

fn read_u8(bytes: &[u8], start: usize) -> Result<u8, HeapRowRedoPayloadError> {
    bytes
        .get(start)
        .copied()
        .ok_or_else(|| error("heap row redo u8 field is truncated"))
}

fn read_u16(bytes: &[u8], start: usize) -> Result<u16, HeapRowRedoPayloadError> {
    let mut array = [0u8; 2];
    array.copy_from_slice(read_bytes(bytes, start, 2)?);
    Ok(u16::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], start: usize) -> Result<u32, HeapRowRedoPayloadError> {
    let mut array = [0u8; 4];
    array.copy_from_slice(read_bytes(bytes, start, 4)?);
    Ok(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], start: usize) -> Result<u64, HeapRowRedoPayloadError> {
    let mut array = [0u8; 8];
    array.copy_from_slice(read_bytes(bytes, start, 8)?);
    Ok(u64::from_le_bytes(array))
}

fn read_bytes(bytes: &[u8], start: usize, len: usize) -> Result<&[u8], HeapRowRedoPayloadError> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| error("heap row redo field offset overflows payload size"))?;
    bytes
        .get(start..end)
        .ok_or_else(|| error("heap row redo field is truncated"))
}

fn error(message: impl Into<String>) -> HeapRowRedoPayloadError {
    HeapRowRedoPayloadError::new(message)
}
