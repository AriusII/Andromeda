use std::cmp::Ordering;

/// B-Tree key comparator - lexicographic byte-level comparison.
///
/// Compares encoded keys using byte-level comparison, maintaining the
/// order-preservation invariant.
#[derive(Debug, Clone)]
pub struct KeyComparator;

impl KeyComparator {
    /// Compare two encoded keys lexicographically.
    ///
    /// Returns Ordering::Less if lhs < rhs, Equal if lhs == rhs, Greater if lhs > rhs.
    ///
    /// # Invariant
    ///
    /// If Key1 < Key2, then encode(Key1) < encode(Key2) lexicographically.
    pub fn compare(lhs: &[u8], rhs: &[u8]) -> Ordering {
        lhs.cmp(rhs)
    }

    /// Check if two encoded keys are equal.
    pub fn equal(lhs: &[u8], rhs: &[u8]) -> bool {
        lhs == rhs
    }

    /// Compare with an upper bound for range scans.
    pub fn compare_range(key: &[u8], upper_bound: &[u8]) -> Ordering {
        key.cmp(upper_bound)
    }
}
