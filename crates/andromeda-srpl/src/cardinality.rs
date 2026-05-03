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
}
