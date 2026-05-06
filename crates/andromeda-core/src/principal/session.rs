use super::CertificateFingerprint;

const MTLS_SESSION_PREFIX: &str = "mtls:";
const FINGERPRINT_EVIDENCE_LEN: usize = 32;

/// Session token bound to a principal for request tracing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken {
    token: String,
}

impl SessionToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.token
    }

    pub fn is_empty(&self) -> bool {
        self.token.is_empty()
    }

    /// Derive a deterministic token from certificate fingerprint evidence.
    ///
    /// Format: `mtls:{first_32_chars_of_fingerprint_or_full_if_shorter}`.
    /// This is an audit-correlation token, not a cryptographic bearer token.
    pub fn from_certificate_fingerprint(fingerprint: &CertificateFingerprint) -> Self {
        let fingerprint = fingerprint.as_str();
        let truncated = if fingerprint.chars().count() > FINGERPRINT_EVIDENCE_LEN {
            fingerprint
                .chars()
                .take(FINGERPRINT_EVIDENCE_LEN)
                .collect::<String>()
        } else {
            fingerprint.to_string()
        };
        Self::new(format!("{MTLS_SESSION_PREFIX}{truncated}"))
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.token)
    }
}
