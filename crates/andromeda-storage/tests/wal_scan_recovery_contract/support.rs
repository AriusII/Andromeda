use andromeda_core::TransactionId;
use andromeda_storage::publication::DatabaseManifest;
use andromeda_storage::write_ahead_log::codec::encode_wal_record;
use andromeda_storage::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_storage::{
    AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentDescriptor, SegmentHeader,
    SegmentId, SegmentState, SegmentTrailer,
};

pub(crate) fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(42)),
        payload,
    )
    .unwrap()
}

pub(crate) fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: Option<u64>,
    payload: &[u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        transaction_id.map(TransactionId::new),
        payload,
    )
    .unwrap()
}

pub(crate) fn encode_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}

pub(crate) fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

pub(crate) fn segment(state: SegmentState, max_page_lsn: Lsn) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(301),
        object_id: ObjectId::new(302),
        allocation_id: AllocationId::new(303),
        first_page_id: PageId::new(10_000),
        page_count: 16,
        min_page_lsn: Lsn::new(1),
        max_page_lsn,
        header_crc: 304,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(305),
        extent_count: 2,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: match state {
            SegmentState::BuildingHotSnapshot => None,
            SegmentState::Sealed | SegmentState::PublishedCold => Some(306),
        },
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 307,
            segment_hash: [8; 32],
            trailer_crc: 308,
        },
    }
}
