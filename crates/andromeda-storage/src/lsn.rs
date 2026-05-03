#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Lsn(u64);

impl Lsn {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsn_zero_and_next_are_stable() {
        assert!(Lsn::ZERO.is_zero());
        assert_eq!(Lsn::new(41).next(), Lsn::new(42));
    }
}
