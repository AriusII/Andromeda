use andromeda_core::{AndromedaError, AndromedaErrorKind};

/// Categorical reason a shipment was rejected. Variants are deliberately
/// narrow so upstream tooling can map each to a specific operator action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalShipmentRejection {
    /// Source identity has the reserved zero id.
    SourceIdZero,
    /// Target identity has the reserved zero id.
    TargetIdZero,
    /// Source identity is not a primary.
    SourceNotPrimary,
    /// Target identity is not a replica.
    TargetNotReplica,
    /// Replica tail and expected next LSN do not form one contiguous chain.
    ReplicaExpectationMismatch,
    /// Replica tail cannot advance to a valid expected next LSN.
    ReplicaExpectationOverflow,
    /// Empty batch — shipping must carry at least one record.
    EmptyBatch,
    /// First record's LSN does not match the replica's expected next LSN.
    UnexpectedFirstLsn,
    /// First record's `previous_lsn` does not link to the replica's prior tail.
    PreviousLsnMismatch,
    /// LSN went backwards or stayed equal between adjacent records (reorder
    /// or duplicate).
    NonMonotonicLsn,
    /// Adjacent record's `previous_lsn` does not equal the prior record's LSN
    /// (gap in the chain).
    ChainGap,
    /// A record failed structural self-validation (checksum, payload, etc.).
    RecordSelfInvalid,
}

impl WalShipmentRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceIdZero => "wal shipment source id must not be zero",
            Self::TargetIdZero => "wal shipment target id must not be zero",
            Self::SourceNotPrimary => "wal shipment source is not a primary",
            Self::TargetNotReplica => "wal shipment target is not a replica",
            Self::ReplicaExpectationMismatch => {
                "wal shipment replica expectation does not match the replica tail"
            }
            Self::ReplicaExpectationOverflow => {
                "wal shipment replica expectation would overflow the LSN space"
            }
            Self::EmptyBatch => "wal shipment batch is empty",
            Self::UnexpectedFirstLsn => {
                "wal shipment first LSN does not match replica expected next LSN"
            }
            Self::PreviousLsnMismatch => {
                "wal shipment first record previous_lsn does not link to replica tail"
            }
            Self::NonMonotonicLsn => "wal shipment contains duplicate or reordered LSNs",
            Self::ChainGap => "wal shipment contains a gap in the previous_lsn chain",
            Self::RecordSelfInvalid => "wal shipment contains a structurally invalid record",
        }
    }

    pub(super) fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}
