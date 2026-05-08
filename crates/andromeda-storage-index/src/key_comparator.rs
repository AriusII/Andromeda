use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct KeyComparator;

impl KeyComparator {
    pub fn compare(lhs: &[u8], rhs: &[u8]) -> Ordering {
        lhs.cmp(rhs)
    }

    pub fn equal(lhs: &[u8], rhs: &[u8]) -> bool {
        lhs == rhs
    }

    pub fn compare_range(key: &[u8], upper_bound: &[u8]) -> Ordering {
        key.cmp(upper_bound)
    }
}

#[cfg(test)]
mod tests {
    use super::KeyComparator;
    use std::cmp::Ordering;

    #[test]
    fn comparator_uses_lexicographic_byte_order() {
        assert_eq!(KeyComparator::compare(b"a", b"b"), Ordering::Less);
        assert_eq!(KeyComparator::compare(b"b", b"a"), Ordering::Greater);
        assert!(KeyComparator::equal(b"same", b"same"));
        assert_eq!(KeyComparator::compare_range(b"a", b"z"), Ordering::Less);
    }
}
