use std::fmt;

/// Certificate fingerprint used for mTLS identity binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CertificateFingerprint(String);

impl CertificateFingerprint {
    /// Returns `None` when the input is empty or whitespace only.
    pub fn new(fingerprint: impl Into<String>) -> Option<Self> {
        let fp = fingerprint.into();
        if fp.trim().is_empty() {
            None
        } else {
            Some(Self(fp))
        }
    }

    /// Create a fingerprint without validation (tests/dev).
    pub fn new_unchecked(fingerprint: impl Into<String>) -> Self {
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

impl From<String> for CertificateFingerprint {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for CertificateFingerprint {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
