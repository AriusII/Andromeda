const REDACTED_AUDIT_REASON: &str = "[redacted sensitive audit evidence]";
const UNSPECIFIED_AUDIT_REASON: &str = "unspecified audit evidence";

const SENSITIVE_AUDIT_MARKERS: &[&str] = &[
    "authorization:",
    "bearer ",
    "credential=",
    "password=",
    "private key",
    "private_key",
    "secret=",
    "token=",
    "x-api-key",
];

pub fn redact_audit_reason(reason: impl Into<String>) -> String {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return UNSPECIFIED_AUDIT_REASON.to_string();
    }
    if audit_text_contains_sensitive_marker(&reason) {
        return REDACTED_AUDIT_REASON.to_string();
    }
    reason
}

pub fn audit_text_contains_sensitive_marker(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    SENSITIVE_AUDIT_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}
