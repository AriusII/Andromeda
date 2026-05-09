use std::fmt;

/// Certificate fingerprint used for mTLS identity binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CertificateFingerprint(String);

impl CertificateFingerprint {
    /// Stores trimmed certificate fingerprint evidence.
    ///
    /// Returns `None` when the trimmed input is empty. Legacy non-SHA evidence
    /// remains accepted here for compatibility; current mTLS identity binding
    /// performs stricter SHA-256 validation at the certificate identity layer.
    pub fn new(fingerprint: impl Into<String>) -> Option<Self> {
        let fingerprint = fingerprint.into();
        let fingerprint = fingerprint.trim();
        if fingerprint.is_empty() {
            None
        } else {
            Some(Self(fingerprint.to_string()))
        }
    }

    #[cfg(test)]
    pub(crate) fn new_unchecked(fingerprint: impl Into<String>) -> Self {
        Self(fingerprint.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the fingerprint is a 64-char ASCII hex string.
    pub fn is_valid_sha256(&self) -> bool {
        self.0.len() == 64 && self.0.chars().all(|c| c.is_ascii_hexdigit())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CertificateFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
