use andromeda_core::EngineTimestamp;

use super::ScenarioEvidenceError;

/// Half-open validity window `[issued_at, expires_at)`.
///
/// `issued_at` must be strictly before `expires_at`; `expires_at == 0` is
/// rejected so the "absent expiry" sentinel cannot accidentally make every
/// record valid forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidityWindow {
    issued_at: EngineTimestamp,
    expires_at: EngineTimestamp,
}

impl ValidityWindow {
    /// Construct a window. Validates `issued_at < expires_at` and
    /// `expires_at != 0`.
    pub fn new(
        issued_at: EngineTimestamp,
        expires_at: EngineTimestamp,
    ) -> Result<Self, ScenarioEvidenceError> {
        if expires_at.is_zero() {
            return Err(ScenarioEvidenceError::ExpiryMustBeNonZero);
        }
        if issued_at.as_unix_millis() >= expires_at.as_unix_millis() {
            return Err(ScenarioEvidenceError::IssuedNotBeforeExpiry);
        }
        Ok(Self {
            issued_at,
            expires_at,
        })
    }

    pub const fn issued_at(self) -> EngineTimestamp {
        self.issued_at
    }

    pub const fn expires_at(self) -> EngineTimestamp {
        self.expires_at
    }

    /// `true` when `now >= expires_at`. No grace period; consumers that want
    /// one must apply it explicitly *before* calling.
    pub fn is_expired_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() >= self.expires_at.as_unix_millis()
    }

    /// `true` when `now < issued_at`. Records issued in the future are not yet
    /// usable; surfacing this as a distinct condition prevents a silent "treat
    /// as valid" path.
    pub fn is_not_yet_valid_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() < self.issued_at.as_unix_millis()
    }
}
