use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Lsn(u64);

impl Lsn {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

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

    pub const fn checked_next(self) -> Option<Self> {
        if self.0 == u64::MAX {
            None
        } else {
            Some(Self(self.0 + 1))
        }
    }

    pub fn try_next(self) -> AndromedaResult<Self> {
        self.checked_next().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "LSN advancement would overflow u64",
            )
        })
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

    #[test]
    fn lsn_checked_next_rejects_overflow() {
        assert_eq!(Lsn::new(41).checked_next(), Some(Lsn::new(42)));
        assert!(Lsn::MAX.checked_next().is_none());
        assert_eq!(
            Lsn::MAX.try_next().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
