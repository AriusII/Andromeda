use super::StreamCompletion;
use andromeda_proto::StructuredObjectHeader;

#[derive(Debug)]
pub(super) enum ResultStreamMessage {
    Row(StructuredObjectHeader),
    Completion(StreamCompletion),
}

impl ResultStreamMessage {
    pub(super) fn row(row: StructuredObjectHeader) -> Self {
        Self::Row(row)
    }

    pub(super) const fn completion(completion: StreamCompletion) -> Self {
        Self::Completion(completion)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResultStreamFrame {
    Row(StructuredObjectHeader),
    Completion(StreamCompletion),
}
