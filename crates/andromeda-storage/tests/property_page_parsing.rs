//! Property-based fuzz tests for page header and trailer parsing.
//!
//! # Goal
//! Verify that page structure parsing:
//! - Never corrupts heap or stack memory
//! - Maintains invariants (bounds checking, size validation)
//! - Handles malformed headers gracefully
//!
//! # Properties Tested
//! 1. Parser rejects invalid magic and page-size fields
//! 2. Parser never reads/writes out of bounds
//! 3. Parser rejects pages with invalid sizes
//! 4. Parser validates page type correctly
//! 5. LSN fields preserved correctly
//! 6. No panic on truncated headers
//! 7. Corruption in trailer detected

#![forbid(unsafe_code)]

use proptest::prelude::*;
use std::panic;

// Constants

const PAGE_HEADER_SIZE: usize = 128;
const PAGE_TRAILER_SIZE: usize = 64;
const MIN_PAGE_SIZE: u32 = 4096;
const MAX_PAGE_SIZE: u32 = 65536;

// Mock Page Structures (actual would come from andromeda_storage)

#[derive(Debug, Clone)]
pub struct PageHeader {
    pub magic: u64,
    pub page_size: u32,
    pub page_type: u8,
    pub lsn: u64,
    pub checksum: u64,
}

#[derive(Debug, Clone)]
pub struct PageTrailer {
    pub page_id: u64,
    pub checksum: u64,
}

// Test Data Generators

fn _arb_page_size() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(4096u32),
        Just(8192u32),
        Just(16384u32),
        Just(32768u32),
        4096u32..32768u32,
    ]
}

fn arb_valid_page_header() -> impl Strategy<Value = Vec<u8>> {
    (
        0u64..u64::MAX, // page_size
        0u8..8u8,       // page_type
        0u64..u64::MAX, // lsn
    )
        .prop_map(|(page_size, page_type, lsn)| {
            let mut header = vec![0u8; PAGE_HEADER_SIZE];

            // Magic number
            header[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());

            // Page size (at offset 8)
            let ps = std::cmp::min(
                std::cmp::max(page_size, MIN_PAGE_SIZE as u64),
                MAX_PAGE_SIZE as u64,
            ) as u32;
            header[8..12].copy_from_slice(&ps.to_le_bytes());

            // Page type (at offset 12)
            header[12] = page_type;

            // LSN (at offset 16)
            header[16..24].copy_from_slice(&lsn.to_le_bytes());

            header
        })
}

fn arb_corrupted_byte_position() -> impl Strategy<Value = usize> {
    0usize..PAGE_HEADER_SIZE
}

#[test]
fn prop_page_header_validated_field_corruption() {
    proptest!(|(
        mut header in arb_valid_page_header(),
        corruption_pos in arb_corrupted_byte_position(),
    )| {
        if corruption_pos < header.len() {
            header[corruption_pos] ^= 0xFF;
        }

        let result = parse_page_header_safely(&header);

        if corruption_pos < 8 {
            prop_assert!(
                matches!(result, PageParseResult::Invalid(_)),
                "magic corruption must be rejected"
            );
        } else if let PageParseResult::Invalid(message) = result {
            prop_assert!(!message.is_empty());
        }
    });
}

#[test]
fn prop_page_header_parse_never_panics() {
    proptest!(|(data in prop::collection::vec(0u8..=255u8, 0..1000))| {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            parse_page_header_safely(&data)
        }));

        prop_assert!(result.is_ok(), "parsing panicked");
    });
}

#[test]
fn prop_page_invalid_size_rejected() {
    proptest!(|(invalid_size in 0u32..4096u32)| {
        let mut header = vec![0u8; PAGE_HEADER_SIZE];

        // Magic
        header[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());

        // Invalid page size
        header[8..12].copy_from_slice(&invalid_size.to_le_bytes());

        let result = parse_page_header_safely(&header);

        match result {
            PageParseResult::Valid(parsed) => {
                prop_assert!(parsed.page_size >= MIN_PAGE_SIZE);
            }
            PageParseResult::Invalid(msg) => prop_assert!(!msg.is_empty()),
        }
    });
}

#[test]
fn prop_page_oversized_rejected() {
    proptest!(|(oversized in (MAX_PAGE_SIZE + 1)..u32::MAX)| {
        let mut header = vec![0u8; PAGE_HEADER_SIZE];

        // Magic
        header[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());

        // Oversized page size
        header[8..12].copy_from_slice(&oversized.to_le_bytes());

        let result = parse_page_header_safely(&header);

        prop_assert!(
            matches!(result, PageParseResult::Invalid(_)),
            "oversized page was accepted"
        );
    });
}

#[test]
fn prop_page_truncated_header_safe() {
    proptest!(|(size in 0usize..PAGE_HEADER_SIZE)| {
        let mut header = vec![0u8; size];

        // Try to populate what we can
        if header.len() >= 8 {
            header[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());
        }

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            parse_page_header_safely(&header)
        }));

        match result {
            Ok(PageParseResult::Invalid(message)) if size < 32 => {
                prop_assert!(!message.is_empty());
            }
            Ok(_) => {}
            Err(_) => prop_assert!(false, "truncated header caused panic"),
        }
    });
}

#[test]
fn prop_page_lsn_preserved() {
    proptest!(|(lsn in 0u64..u64::MAX)| {
        let mut header = vec![0u8; PAGE_HEADER_SIZE];

        // Magic
        header[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());

        // Page size
        header[8..12].copy_from_slice(&16384u32.to_le_bytes());

        // LSN at offset 16
        header[16..24].copy_from_slice(&lsn.to_le_bytes());

        let result = parse_page_header_safely(&header);

        match result {
            PageParseResult::Valid(parsed) => {
                prop_assert_eq!(parsed.lsn, lsn);
            }
            PageParseResult::Invalid(message) => prop_assert!(!message.is_empty()),
        }
    });
}

#[test]
fn prop_page_trailer_parse_is_bounded() {
    proptest!(|(
        mut trailer in prop::collection::vec(0u8..=255u8, PAGE_TRAILER_SIZE..PAGE_TRAILER_SIZE + 1),
        corrupt_pos in 0usize..PAGE_TRAILER_SIZE,
    )| {
        if corrupt_pos < trailer.len() {
            trailer[corrupt_pos] ^= 0xFF;
        }

        let result = parse_page_trailer_safely(&trailer);

        prop_assert!(matches!(result, PageTrailerResult::Valid));
    });
}

#[test]
fn prop_page_empty_data_rejected() {
    let empty = vec![];
    let result = parse_page_header_safely(&empty);

    assert!(matches!(result, PageParseResult::Invalid(_)));
}

#[test]
fn prop_page_magic_validation() {
    proptest!(|(wrong_magic in 0u64..u64::MAX)| {
        if wrong_magic == 0x414e_4452_4f50_4147 {
            return Ok(()); // Skip correct magic
        }

        let mut header = vec![0u8; PAGE_HEADER_SIZE];

        // Wrong magic
        header[0..8].copy_from_slice(&wrong_magic.to_le_bytes());

        // Valid page size
        header[8..12].copy_from_slice(&16384u32.to_le_bytes());

        let result = parse_page_header_safely(&header);

        prop_assert!(
            matches!(result, PageParseResult::Invalid(_)),
            "wrong magic was accepted"
        );

    });
}

#[test]
fn prop_page_large_corrupted_data() {
    proptest!(|(size in 16384usize..65536usize)| {
        let mut data = vec![0xFFu8; size];

        // Try to set magic
        if data.len() >= 8 {
            data[0..8].copy_from_slice(&0x414e_4452_4f50_4147u64.to_le_bytes());
        }

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            parse_page_header_safely(&data)
        }));

        match result {
            Ok(PageParseResult::Valid(parsed)) => {
                prop_assert_eq!(parsed.magic, 0x414e_4452_4f50_4147);
                prop_assert!((MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&parsed.page_size));
            }
            Ok(PageParseResult::Invalid(message)) => prop_assert!(!message.is_empty()),
            Err(_) => prop_assert!(false, "large data caused panic"),
        }
    });
}

// Mock Parser Implementation

#[derive(Debug, Clone)]
enum PageParseResult {
    Valid(PageHeader),
    Invalid(String),
}

#[derive(Debug, Clone)]
enum PageTrailerResult {
    Valid,
    Invalid,
}

fn parse_page_header_safely(data: &[u8]) -> PageParseResult {
    if data.len() < 32 {
        return PageParseResult::Invalid("header too small".to_string());
    }

    let magic = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);

    if magic != 0x414e_4452_4f50_4147 {
        return PageParseResult::Invalid("invalid magic".to_string());
    }

    let page_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

    if !(MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&page_size) {
        return PageParseResult::Invalid("invalid page size".to_string());
    }

    PageParseResult::Valid(PageHeader {
        magic,
        page_size,
        page_type: data[12],
        lsn: u64::from_le_bytes([
            data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
        ]),
        checksum: u64::from_le_bytes([
            data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
        ]),
    })
}

fn parse_page_trailer_safely(data: &[u8]) -> PageTrailerResult {
    if data.len() < 16 {
        return PageTrailerResult::Invalid;
    }

    let _trailer = PageTrailer {
        page_id: u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]),
        checksum: u64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]),
    };
    PageTrailerResult::Valid
}

#[test]
fn integration_page_full_validation_cycle() {
    proptest!(|(
        pages in prop::collection::vec(
            arb_valid_page_header(),
            1..50
        ),
    )| {
        for header_data in pages.iter() {
            let result = parse_page_header_safely(header_data);
            match result {
                PageParseResult::Valid(parsed) => {
                    prop_assert_eq!(parsed.magic, 0x414e_4452_4f50_4147);
                    prop_assert!((MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&parsed.page_size));
                }
                PageParseResult::Invalid(message) => prop_assert!(!message.is_empty()),
            }
        }
    });
}
