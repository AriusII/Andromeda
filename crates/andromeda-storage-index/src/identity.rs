/// Unique identifier for a row, comprising a compact heap locator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(u64);

impl RowId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Unique identifier for an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexId(u64);

impl IndexId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Column identifier for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(u32);

impl ColumnId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{ColumnId, IndexId, RowId};

    #[test]
    fn identity_newtypes_round_trip_raw_values() {
        assert_eq!(RowId::new(42).get(), 42);
        assert_eq!(IndexId::new(7).get(), 7);
        assert_eq!(ColumnId::new(3).get(), 3);
    }

    #[test]
    fn identity_newtypes_are_orderable_and_distinct() {
        assert!(RowId::new(1) < RowId::new(2));
        assert!(IndexId::new(10) > IndexId::new(9));
        assert_eq!(ColumnId::new(5), ColumnId::new(5));
    }
}
