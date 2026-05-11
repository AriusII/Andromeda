mod codec;
mod column_chunk_directory;
mod digest;
mod error;
mod types;

pub use codec::{
    SEGMENT_INDEX_V0_BYTE_ORDER, SEGMENT_INDEX_V0_ENTRY_LEN,
    SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES, SEGMENT_INDEX_V0_FLAG_FORENSIC_HOLD,
    SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY, SEGMENT_INDEX_V0_FORMAT_MAJOR,
    SEGMENT_INDEX_V0_FORMAT_MINOR, SEGMENT_INDEX_V0_HEADER_LEN, SEGMENT_INDEX_V0_MAGIC,
    SEGMENT_INDEX_V0_MAX_ENTRIES, SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES,
    SEGMENT_INDEX_V0_TRAILER_LEN,
};
pub use column_chunk_directory::{
    COLUMN_CHUNK_DIRECTORY_ENTRY_LEN, COLUMN_CHUNK_DIRECTORY_EXT_FLAGS,
    COLUMN_CHUNK_DIRECTORY_EXT_TYPE, ColumnChunkDirectory, ColumnChunkDirectoryEntry,
};
pub use error::{SegmentIndexError, SegmentIndexResult};
pub use types::{
    SegmentIndexBuildContextV0, SegmentIndexEntryV0, SegmentIndexHeaderV0, SegmentIndexTrailerV0,
    SegmentIndexV0,
};
