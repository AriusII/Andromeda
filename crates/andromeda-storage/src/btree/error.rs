use super::PageId;
use andromeda_core::{AndromedaError, AndromedaResult};

/// B-Tree index errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BTreeError {
    KeyNotFound { key: Vec<u8> },
    DuplicateKey { key: Vec<u8> },
    NodeNotFound { node_id: PageId },
    CorruptedNode { node_id: PageId, reason: String },
    TreeTooDeep { height: u32 },
    InvalidNodeFormat { page_id: PageId },
    BufferPoolError(String),
    SerializationError(String),
}

impl std::fmt::Display for BTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BTreeError::KeyNotFound { key } => {
                write!(f, "key not found: {:?}", String::from_utf8_lossy(key))
            }
            BTreeError::DuplicateKey { key } => {
                write!(f, "duplicate key: {:?}", String::from_utf8_lossy(key))
            }
            BTreeError::NodeNotFound { node_id } => write!(f, "node not found: {:?}", node_id),
            BTreeError::CorruptedNode { node_id, reason } => {
                write!(f, "corrupted node {:?}: {}", node_id, reason)
            }
            BTreeError::TreeTooDeep { height } => write!(f, "tree too deep: height={}", height),
            BTreeError::InvalidNodeFormat { page_id } => {
                write!(f, "invalid node format: page_id={:?}", page_id)
            }
            BTreeError::BufferPoolError(msg) => write!(f, "buffer pool error: {}", msg),
            BTreeError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for BTreeError {}

impl From<BTreeError> for AndromedaError {
    fn from(err: BTreeError) -> Self {
        use andromeda_core::AndromedaErrorKind;
        AndromedaError::new(AndromedaErrorKind::Storage, err.to_string())
    }
}

pub(crate) fn deferred_btree_result<T>(operation: &'static str) -> AndromedaResult<T> {
    use andromeda_core::AndromedaErrorKind;

    Err(AndromedaError::new(
        AndromedaErrorKind::Storage,
        format!(
            "B-Tree durable operation '{operation}' is not promoted; page-backed node format, WAL payload decoding, and idempotent recovery must be implemented before this path can run"
        ),
    ))
}
