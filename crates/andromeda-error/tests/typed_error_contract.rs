use std::{error::Error as StdError, fmt};

use andromeda_error::{AndromedaError, AndromedaErrorKind};

const KIND_CONTRACT: &[(AndromedaErrorKind, &str, &str)] = &[
    (AndromedaErrorKind::Catalog, "AE0001", "catalog"),
    (AndromedaErrorKind::Contract, "AE0002", "contract"),
    (AndromedaErrorKind::Execution, "AE0003", "execution"),
    (AndromedaErrorKind::Internal, "AE0004", "internal"),
    (AndromedaErrorKind::Protocol, "AE0005", "protocol"),
    (AndromedaErrorKind::Resource, "AE0006", "resource"),
    (AndromedaErrorKind::Security, "AE0007", "security"),
    (AndromedaErrorKind::Srpl, "AE0008", "srpl"),
    (AndromedaErrorKind::Storage, "AE0009", "storage"),
    (AndromedaErrorKind::Timeout, "AE0010", "timeout"),
    (AndromedaErrorKind::Transaction, "AE0011", "transaction"),
    (AndromedaErrorKind::Transport, "AE0012", "transport"),
];

#[derive(Debug)]
struct LeafError(&'static str);

impl fmt::Display for LeafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl StdError for LeafError {}

#[derive(Debug)]
struct WrapperError {
    message: &'static str,
    source: LeafError,
}

impl fmt::Display for WrapperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl StdError for WrapperError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.source)
    }
}

fn assert_error_is_not_string_only(error: &AndromedaError, expected_message: &str) {
    assert_ne!(error.to_string(), expected_message);
    assert_ne!(error.render_diagnostic(), expected_message);
    assert!(error.render_diagnostic().starts_with(error.code()));
    assert!(error.render_diagnostic().contains(error.kind().as_str()));
    assert!(error.render_diagnostic().ends_with(expected_message));
}

#[test]
fn stable_error_codes_are_unique_and_bound_to_labels() {
    let mut seen_codes = Vec::new();

    for (kind, expected_code, expected_label) in KIND_CONTRACT {
        assert_eq!(kind.code(), *expected_code);
        assert_eq!(kind.as_str(), *expected_label);
        assert_eq!(kind.to_string(), *expected_label);
        assert!(
            !seen_codes.contains(expected_code),
            "duplicate error code {expected_code}"
        );
        seen_codes.push(*expected_code);
    }
}

#[test]
fn typed_engine_errors_are_never_rendered_as_message_only() {
    for (kind, expected_code, expected_label) in KIND_CONTRACT {
        let message = "critical boundary rejected";
        let error = AndromedaError::new(*kind, message);

        assert_eq!(error.kind(), *kind);
        assert_eq!(error.code(), *expected_code);
        assert_eq!(error.kind().as_str(), *expected_label);
        assert_error_is_not_string_only(&error, message);
    }
}

#[test]
fn source_chaining_preserves_outer_error_and_inner_sources() {
    let source = WrapperError {
        message: "wrapper codec fault",
        source: LeafError("leaf checksum mismatch"),
    };
    let error =
        AndromedaError::with_source(AndromedaErrorKind::Storage, "WAL frame rejected", source);

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(error.code(), "AE0009");
    assert_eq!(error.message(), "WAL frame rejected");

    let source = StdError::source(&error).expect("AndromedaError should preserve source");
    assert_eq!(source.to_string(), "wrapper codec fault");
    assert_eq!(
        source
            .source()
            .expect("wrapper source should preserve leaf")
            .to_string(),
        "leaf checksum mismatch"
    );
}

#[test]
fn diagnostic_rendering_is_deterministic() {
    let source = WrapperError {
        message: "wrapper codec fault",
        source: LeafError("leaf checksum mismatch"),
    };
    let error =
        AndromedaError::with_source(AndromedaErrorKind::Storage, "WAL frame rejected", source);

    let expected = concat!(
        "AE0009 storage: WAL frame rejected",
        "; source: wrapper codec fault",
        "; source: leaf checksum mismatch"
    );

    assert_eq!(error.render_diagnostic(), expected);
    assert_eq!(error.render_diagnostic(), expected);
}
