/// Maximum size for a single WAL record payload (1 MB).
///
/// This prevents unbounded allocation during record encoding and decoding.
pub const WAL_RECORD_SIZE_LIMIT: u64 = 1024 * 1024;

/// Maximum number of row operations per transaction batch.
pub const WAL_BATCH_ROW_LIMIT: usize = 256;

/// Segment boundary size (4 MB default segment).
///
/// Cumulative encoded record size must not exceed this boundary.
pub const WAL_SEGMENT_BOUNDARY: u64 = 4 * 1024 * 1024;

/// Maximum WAL header size used for conservative encoded-size accounting.
pub const WAL_RECORD_HEADER_OVERHEAD: u64 = 72;
