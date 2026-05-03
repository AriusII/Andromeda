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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinality_exposes_exact_count_need() {
        assert!(Cardinality::One.requires_exact_row_count());
        assert!(Cardinality::NonEmptyMany.requires_exact_row_count());
        assert!(!Cardinality::Many.requires_exact_row_count());
    }
}
