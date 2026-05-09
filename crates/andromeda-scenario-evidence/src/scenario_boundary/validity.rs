use andromeda_time::EngineTimestamp;

use crate::MAX_EVIDENCE_TTL_MS;

use super::errors::BenchmarkScenarioEvidenceError;

/// Half-open validity window for benchmark evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceValidity {
    issued_at: EngineTimestamp,
    expires_at: EngineTimestamp,
}

impl BenchmarkEvidenceValidity {
    pub fn new(
        issued_at: EngineTimestamp,
        expires_at: EngineTimestamp,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if expires_at.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::ExpiryMustBeNonZero);
        }
        if issued_at.as_unix_millis() >= expires_at.as_unix_millis() {
            return Err(BenchmarkScenarioEvidenceError::IssuedNotBeforeExpiry);
        }
        let ttl_ms = expires_at.as_unix_millis() - issued_at.as_unix_millis();
        if ttl_ms > MAX_EVIDENCE_TTL_MS {
            return Err(BenchmarkScenarioEvidenceError::ValidityWindowExceedsLimit);
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

    pub fn is_not_yet_valid_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() < self.issued_at.as_unix_millis()
    }

    pub fn is_expired_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() >= self.expires_at.as_unix_millis()
    }
}
