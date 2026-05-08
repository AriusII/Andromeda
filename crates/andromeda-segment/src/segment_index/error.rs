use std::fmt::{Display, Formatter};

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub type SegmentIndexResult<T> = Result<T, SegmentIndexError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentIndexError {
    Truncated { field: &'static str },
    LengthOverflow { field: &'static str },
    UnsupportedVersion { major: u16, minor: u16 },
    InvalidHeader { reason: &'static str },
    InvalidEntry { index: usize, reason: &'static str },
    InvalidOrdering { index: usize, reason: &'static str },
    InvalidExtension { reason: &'static str },
    HeaderChecksumMismatch,
    EntryChecksumMismatch { index: usize },
    TrailerChecksumMismatch,
    EntryTableChecksumMismatch,
    EntryTableDigestMismatch,
    FileDigestMismatch,
    RootDigestMismatch,
}

impl Display for SegmentIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated { field } => write!(f, "segment index field is truncated: {field}"),
            Self::LengthOverflow { field } => {
                write!(f, "segment index length overflows: {field}")
            },
            Self::UnsupportedVersion { major, minor } => {
                write!(
                    f,
                    "unsupported segment index format version: {major}.{minor}"
                )
            },
            Self::InvalidHeader { reason } => write!(f, "invalid segment index header: {reason}"),
            Self::InvalidEntry { index, reason } => {
                write!(f, "invalid segment index entry {index}: {reason}")
            },
            Self::InvalidOrdering { index, reason } => {
                write!(
                    f,
                    "invalid segment index ordering at entry {index}: {reason}"
                )
            },
            Self::InvalidExtension { reason } => {
                write!(f, "invalid segment index extension: {reason}")
            },
            Self::HeaderChecksumMismatch => write!(f, "segment index header CRC mismatch"),
            Self::EntryChecksumMismatch { index } => {
                write!(f, "segment index entry CRC mismatch at entry {index}")
            },
            Self::TrailerChecksumMismatch => write!(f, "segment index trailer CRC mismatch"),
            Self::EntryTableChecksumMismatch => write!(f, "segment index entry table CRC mismatch"),
            Self::EntryTableDigestMismatch => {
                write!(f, "segment index entry table digest mismatch")
            },
            Self::FileDigestMismatch => write!(f, "segment index file digest mismatch"),
            Self::RootDigestMismatch => write!(f, "segment index root digest mismatch"),
        }
    }
}

impl std::error::Error for SegmentIndexError {}

impl From<SegmentIndexError> for AndromedaError {
    fn from(err: SegmentIndexError) -> Self {
        Self::new(AndromedaErrorKind::Storage, err.to_string())
    }
}
