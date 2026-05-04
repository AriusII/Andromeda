#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    One,
    OptionalOne,
    Many,
    NonEmptyMany,
}

impl Cardinality {
    pub const fn requires_exact_row_count(self) -> bool {
        matches!(self, Self::One | Self::OptionalOne | Self::NonEmptyMany)
    }

    pub const fn permits_exact_row_count(self, row_count: u64) -> bool {
        match self {
            Self::One => row_count == 1,
            Self::OptionalOne => row_count <= 1,
            Self::Many => true,
            Self::NonEmptyMany => row_count >= 1,
        }
    }

    /// Lower bound on the number of rows a stream of this cardinality may
    /// emit. Used by the result-cardinality contract to validate declared
    /// upper bounds (`row_count_max`) and post-stream actual row counts.
    pub const fn min_row_count(self) -> u64 {
        match self {
            Self::One | Self::NonEmptyMany => 1,
            Self::OptionalOne | Self::Many => 0,
        }
    }

    /// Intrinsic upper bound carried by the cardinality kind itself.
    /// `One` and `OptionalOne` are intrinsically bounded at 1 row.
    /// `Many` and `NonEmptyMany` have no intrinsic upper bound and rely on
    /// an explicit `row_count_max` from the contract surface to be bounded.
    pub const fn intrinsic_max_row_count(self) -> Option<u64> {
        match self {
            Self::One | Self::OptionalOne => Some(1),
            Self::Many | Self::NonEmptyMany => None,
        }
    }

    /// True when an explicit upper bound `row_count_max` is consistent with
    /// this cardinality (i.e. it neither contradicts the intrinsic max nor
    /// the minimum row count).
    pub const fn permits_row_count_max(self, row_count_max: u64) -> bool {
        if row_count_max < self.min_row_count() {
            return false;
        }
        match self.intrinsic_max_row_count() {
            Some(intrinsic) => row_count_max <= intrinsic,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinality_exposes_exact_count_need() {
        assert!(Cardinality::One.requires_exact_row_count());
        assert!(Cardinality::OptionalOne.requires_exact_row_count());
        assert!(Cardinality::NonEmptyMany.requires_exact_row_count());
        assert!(!Cardinality::Many.requires_exact_row_count());
    }

    #[test]
    fn cardinality_validates_exact_counts_when_available() {
        assert!(Cardinality::One.permits_exact_row_count(1));
        assert!(!Cardinality::One.permits_exact_row_count(0));
        assert!(!Cardinality::One.permits_exact_row_count(2));
        assert!(Cardinality::OptionalOne.permits_exact_row_count(0));
        assert!(Cardinality::OptionalOne.permits_exact_row_count(1));
        assert!(!Cardinality::OptionalOne.permits_exact_row_count(2));
        assert!(Cardinality::NonEmptyMany.permits_exact_row_count(1));
        assert!(!Cardinality::NonEmptyMany.permits_exact_row_count(0));
        assert!(Cardinality::Many.permits_exact_row_count(0));
    }

    #[test]
    fn cardinality_exposes_min_and_intrinsic_max_row_counts() {
        assert_eq!(Cardinality::One.min_row_count(), 1);
        assert_eq!(Cardinality::OptionalOne.min_row_count(), 0);
        assert_eq!(Cardinality::NonEmptyMany.min_row_count(), 1);
        assert_eq!(Cardinality::Many.min_row_count(), 0);

        assert_eq!(Cardinality::One.intrinsic_max_row_count(), Some(1));
        assert_eq!(Cardinality::OptionalOne.intrinsic_max_row_count(), Some(1));
        assert_eq!(Cardinality::NonEmptyMany.intrinsic_max_row_count(), None);
        assert_eq!(Cardinality::Many.intrinsic_max_row_count(), None);
    }

    #[test]
    fn cardinality_row_count_max_respects_intrinsic_and_min_bounds() {
        // One/OptionalOne: max must be exactly 1 (must be >= min and <= intrinsic).
        assert!(Cardinality::One.permits_row_count_max(1));
        assert!(!Cardinality::One.permits_row_count_max(0));
        assert!(!Cardinality::One.permits_row_count_max(2));
        assert!(Cardinality::OptionalOne.permits_row_count_max(1));
        // OptionalOne has min 0, so a declared max of 0 is permissible (a
        // contractually empty optional-one stream).
        assert!(Cardinality::OptionalOne.permits_row_count_max(0));
        assert!(!Cardinality::OptionalOne.permits_row_count_max(2));

        // NonEmptyMany: any max >= 1 is permitted; 0 is not.
        assert!(!Cardinality::NonEmptyMany.permits_row_count_max(0));
        assert!(Cardinality::NonEmptyMany.permits_row_count_max(1));
        assert!(Cardinality::NonEmptyMany.permits_row_count_max(1_000_000));

        // Many: any max >= 0 is permitted.
        assert!(Cardinality::Many.permits_row_count_max(0));
        assert!(Cardinality::Many.permits_row_count_max(1_000_000));
    }
}
