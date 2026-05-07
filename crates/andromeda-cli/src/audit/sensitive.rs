const REDACTED_AUDIT_EVIDENCE: &str = "[redacted-sensitive-audit-evidence]";
const SENSITIVE_MARKERS: &[&str] = &[
    "token=",
    "bearer ",
    "credential=",
    "x-api-key",
    "private_key",
];

pub(super) fn contains_sensitive_cli_evidence(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

pub(super) fn redact_sensitive_cli_evidence(value: &str) -> &str {
    if contains_sensitive_cli_evidence(value) {
        REDACTED_AUDIT_EVIDENCE
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secret_bearing_cli_evidence_markers() {
        for value in [
            "token=super-secret",
            "Bearer super-secret",
            "credential=super-secret",
            "x-api-key: super-secret",
            "private_key=super-secret",
        ] {
            assert!(
                contains_sensitive_cli_evidence(value),
                "expected sensitive marker in {value}"
            );
            assert_eq!(
                redact_sensitive_cli_evidence(value),
                REDACTED_AUDIT_EVIDENCE
            );
        }
    }

    #[test]
    fn preserves_non_secret_audit_evidence() {
        let value = "user:ops";

        assert!(!contains_sensitive_cli_evidence(value));
        assert_eq!(redact_sensitive_cli_evidence(value), value);
    }
}
