//! P11 exit criterion runtime integration test — WAL queue separation.
//!
//! **P11 line 72:** *"P0 WAL flush ne partage pas la même file logique que temp/spill."*
//!
//! This test proves at runtime (not just at the type-contract level) that:
//!
//! 1. [`andromeda_wal::FileWal::flush_through`] produces
//!    [`andromeda_wal::WalQueueSeparationEvidence`] with
//!    `shares_queue_with_temp_spill: false` and `queue_class: P0Durability`.
//! 2. [`andromeda_disk_page_store::FileDiskManager::atomic_write_page`] (invoked
//!    via `write_page`) is classified [`andromeda_wal::WalIoQueueClass::P1Maintenance`]
//!    by default — never `P0Durability`.
//! 3. Concurrent WAL flush + N temp/spill page writes on the **same temp directory**
//!    (same-device scenario) produce evidence that the logical queues are NOT shared.
//! 4. The negative path: configuring a shared P0 queue causes `flush_through` to
//!    return a typed error, not panic.
//! 5. All of the above holds across 5 independent runs (non-flakiness evidence).
//!
//! # Test location note
//! This test lives in `andromeda-disk-page-store` rather than `andromeda-wal` because:
//! - `andromeda-disk-page-store` already depends on `andromeda-wal`.
//! - Adding `andromeda-disk-page-store` as a dev-dependency of `andromeda-wal`
//!   would create a circular dependency (rejected by Cargo).
//! - Documented in `.work/copilot-cli/p11/missions/W1_REPORT.md`.

#![forbid(unsafe_code)]

use andromeda_disk_page_store::{DiskManager, FileDiskManager};
use andromeda_segment::{ExtentDescriptor, ExtentId, ExtentState};
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract,
    PageSize, PageTrailer, PageType, integrity_trailer_for_payload,
};
use andromeda_wal::{FileWal, WalIoQueueClass, WalQueueSeparationEvidence, WalRecordKind};
use tempfile::TempDir;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn make_single_page_extent(first_page_id: u64) -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(first_page_id),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(first_page_id),
        page_count: 4,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    }
}

/// Build a minimal valid page image for `page_id` at `page_lsn`.
///
/// Computes the integrity trailer via `integrity_trailer_for_payload` so that
/// `PageLayoutContract::validate()` accepts the image.
fn make_test_page(page_id: PageId, page_lsn: Lsn) -> PageImage {
    let header_len = PageHeader::MIN_HEADER_LEN_V0;
    let trailer_len = PageTrailer::V0_LEN;
    let page_bytes = PageSize::KiB16.bytes();
    let available_payload = page_bytes - u32::from(header_len) - trailer_len;

    let header = PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id,
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        page_lsn,
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len,
        payload_offset: u32::from(header_len),
        payload_len: available_payload,
        free_start: u32::from(header_len),
        free_end: u32::from(header_len) + available_payload,
        free_bytes: available_payload,
        slot_count: 0,
        row_count: 0,
        flags: PageFlags::NONE,
        header_crc: 0xDEAD_BEEF,
    };
    let bytes = vec![0xABu8; PageSize::KiB16.bytes_usize()];
    let payload_start = header.payload_offset as usize;
    let payload_end = payload_start + header.payload_len as usize;
    let trailer = integrity_trailer_for_payload(&header, &bytes[payload_start..payload_end]);
    let layout = PageLayoutContract { header, trailer };
    PageImage::with_layout(layout, bytes)
        .expect("valid page image construction must not fail")
}

// ─── core scenario ────────────────────────────────────────────────────────────

/// Run one full separation scenario using a fresh temp directory.
///
/// Steps:
/// 1. Open `FileWal` in `temp_dir` — default separation evidence is P0 + not shared.
/// 2. Verify pre-flush evidence has `shares_queue_with_temp_spill = false`.
/// 3. Spawn N threads, each creating a separate `FileDiskManager` instance in the
///    **same** `temp_dir` (same-device scenario) and writing one page.
/// 4. Concurrently: flush the WAL in the main thread.
/// 5. Assert post-flush evidence still reports no temp/spill sharing.
/// 6. Assert all temp writers completed with `P1Maintenance` classification.
fn run_separation_scenario(temp: &TempDir, iteration: usize) {
    let wal_path = temp.path().join(format!("sep_test_{iteration}.wal"));
    let mut wal =
        FileWal::open(&wal_path).unwrap_or_else(|e| panic!("iter {iteration}: open FileWal: {e}"));

    // ── 1. pre-flush: evidence must be P0Durable + not shared ─────────────
    let pre_evidence = wal.flush_separation_evidence();
    assert_eq!(
        pre_evidence.queue_class,
        WalIoQueueClass::P0Durability,
        "iter {iteration}: flush evidence must be P0Durability by default"
    );
    assert!(
        !pre_evidence.shares_queue_with_temp_spill,
        "iter {iteration}: P0 WAL flush must not share queue with temp/spill (default)"
    );
    pre_evidence
        .validate()
        .unwrap_or_else(|e| panic!("iter {iteration}: pre-flush evidence validate: {e}"));

    // ── 2. append a WAL record ─────────────────────────────────────────────
    wal.append_payload(WalRecordKind::PageFormat, None, b"p11-sep-test".to_vec())
        .unwrap_or_else(|e| panic!("iter {iteration}: append WAL record: {e}"));
    let target_lsn = wal
        .last_lsn()
        .unwrap_or_else(|| panic!("iter {iteration}: last_lsn must be Some after append"));

    // ── 3. spawn N temp writers sharing the same temp_dir ─────────────────
    const N_SPILL_WRITERS: usize = 3;
    let temp_dir_path = temp.path().to_owned();

    let handle = std::thread::spawn(move || {
        for i in 0..N_SPILL_WRITERS {
            let data_file = temp_dir_path.join(format!("spill_{iteration}_{i}.bin"));
            let mut mgr = FileDiskManager::open(&data_file, &temp_dir_path)
                .unwrap_or_else(|e| panic!("spill iter {iteration} / {i}: open manager: {e}"));

            // Classification MUST be P1Maintenance by default — never P0Durability.
            assert_eq!(
                mgr.temp_write_queue_class(),
                WalIoQueueClass::P1Maintenance,
                "spill iter {iteration} / {i}: temp_write_queue_class must default to P1Maintenance"
            );

            // Allocate extent and write one page.
            mgr.allocate_extent(make_single_page_extent(1))
                .unwrap_or_else(|e| panic!("spill iter {iteration} / {i}: allocate_extent: {e}"));

            let page = make_test_page(PageId::new(1), Lsn::new(1));
            mgr.write_page(page, Lsn::new(1))
                .unwrap_or_else(|e| panic!("spill iter {iteration} / {i}: write_page: {e}"));

            // Telemetry counter must advance after the write.
            assert_eq!(
                mgr.temp_write_count(),
                1,
                "spill iter {iteration} / {i}: temp_write_count must be 1 after one write"
            );

            // Re-assert: classification has not drifted.
            assert_eq!(
                mgr.temp_write_queue_class(),
                WalIoQueueClass::P1Maintenance,
                "spill iter {iteration} / {i}: temp_write_queue_class must remain P1Maintenance after write"
            );
        }
    });

    // ── 4. WAL flush concurrently in main thread ───────────────────────────
    wal.flush_through(target_lsn)
        .unwrap_or_else(|e| panic!("iter {iteration}: flush_through: {e}"));

    // ── 5. post-flush: evidence still P0 + not shared ─────────────────────
    let post_evidence = wal.flush_separation_evidence();
    assert_eq!(
        post_evidence.queue_class,
        WalIoQueueClass::P0Durability,
        "iter {iteration}: post-flush evidence must still be P0Durability"
    );
    assert!(
        !post_evidence.shares_queue_with_temp_spill,
        "iter {iteration}: post-flush evidence must still report no temp/spill sharing"
    );
    post_evidence
        .validate()
        .unwrap_or_else(|e| panic!("iter {iteration}: post-flush evidence validate: {e}"));

    // ── 6. join temp writers ───────────────────────────────────────────────
    handle
        .join()
        .unwrap_or_else(|_| panic!("iter {iteration}: spill writer thread panicked"));
}

// ─── tests ────────────────────────────────────────────────────────────────────

/// P11 line 72 runtime assertion: 5 independent runs proving WAL P0 flush and
/// temp/spill writes operate on separate logical queues.
///
/// Each run gets a fresh temp directory so there is no state leakage between
/// iterations.  5 runs demonstrate non-flakiness.
#[test]
fn p11_wal_p0_flush_does_not_share_logical_queue_with_temp_spill_five_runs() {
    for iteration in 0..5 {
        let temp =
            TempDir::new().unwrap_or_else(|e| panic!("iter {iteration}: create TempDir: {e}"));
        run_separation_scenario(&temp, iteration);
    }
}

/// Negative path: configuring P0Durability + `shares_queue_with_temp_spill = true`
/// causes `flush_through` to return a typed error — never panic, never silent.
#[test]
fn flush_through_rejects_p0_shared_with_temp_spill_queue_configuration() {
    let temp = TempDir::new().expect("create TempDir");
    let wal_path = temp.path().join("neg_test.wal");
    let mut wal = FileWal::open(&wal_path)
        .expect("open FileWal for negative test")
        .with_flush_separation(WalQueueSeparationEvidence::new(
            WalIoQueueClass::P0Durability,
            true, // violates: P0 must not share with temp/spill
        ));

    wal.append_payload(WalRecordKind::PageFormat, None, b"neg".to_vec())
        .expect("append WAL record for negative test");
    let lsn = wal.last_lsn().expect("last_lsn after append");

    let error = wal
        .flush_through(lsn)
        .expect_err("flush_through must reject P0+shared configuration");

    // Must be a Storage-kind error, not a panic.
    use andromeda_error::AndromedaErrorKind;
    assert_eq!(
        error.kind(),
        AndromedaErrorKind::Storage,
        "queue separation violation must produce a Storage error"
    );
    assert!(
        error.message().contains("separated from temp/spill"),
        "error message must describe the separation violation; got: {}",
        error.message()
    );
}

/// Negative path: `FileDiskManager::with_temp_write_class` rejects P0Durability
/// at construction time — temp/spill managers must never be promoted to P0.
#[test]
fn file_disk_manager_with_p0_class_is_rejected_at_construction() {
    let temp = TempDir::new().expect("create TempDir");
    let data_file = temp.path().join("neg_class.bin");
    let mgr = FileDiskManager::open(&data_file, temp.path()).expect("open manager");

    let error = mgr
        .with_temp_write_class(WalIoQueueClass::P0Durability)
        .expect_err("with_temp_write_class must reject P0Durability");

    use andromeda_error::AndromedaErrorKind;
    assert_eq!(
        error.kind(),
        AndromedaErrorKind::Storage,
        "queue class violation must produce a Storage error"
    );
    assert!(
        error.message().contains("P0"),
        "error message must reference P0; got: {}",
        error.message()
    );
}

/// Verify that `with_temp_write_class(P1Maintenance)` is accepted and readable.
#[test]
fn file_disk_manager_p1_maintenance_class_is_accepted_and_readable() {
    let temp = TempDir::new().expect("create TempDir");
    let data_file = temp.path().join("p1_class.bin");
    let mgr = FileDiskManager::open(&data_file, temp.path())
        .expect("open manager")
        .with_temp_write_class(WalIoQueueClass::P1Maintenance)
        .expect("P1Maintenance must be accepted");

    assert_eq!(
        mgr.temp_write_queue_class(),
        WalIoQueueClass::P1Maintenance,
        "stored class must round-trip through with_temp_write_class"
    );
    assert_eq!(
        mgr.temp_write_count(),
        0,
        "fresh manager must report zero temp writes"
    );
}
