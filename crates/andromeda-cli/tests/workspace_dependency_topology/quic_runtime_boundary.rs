use crate::{diagnostics::rust_source_files, workspace_root};
use std::{
    fs,
    path::{Path, PathBuf},
};

const RUNTIME_FEATURE: &str = "runtime-quinn";
const INSECURE_TEST_TLS_FEATURE: &str = "insecure-test-tls";
const RUNTIME_CRATE: &str = "andromeda-quic-runtime-quinn";
const RUNTIME_DEPS: [&str; 4] = ["quinn", "rcgen", "rustls", "tokio"];
const RUNTIME_QUINN_TEST_TARGETS: [&str; 2] =
    ["real_quinn_network", "reconnect_quinn_admission_contract"];
const QUIC_ROOT_FORBIDDEN_PROJECTION_REEXPORTS: &[&str] = &[
    "CatalogColumnDescriptor",
    "CatalogManifestResolutionRequest",
    "CatalogManifestResolutionResponse",
    "CatalogManifestResolutionStatus",
    "CatalogManifestSelector",
    "CatalogProcedureManifest",
    "CatalogProcedureManifestResolutionRequest",
    "CatalogProcedureManifestResolutionResponse",
    "CatalogProcedureProtocolLayout",
    "CatalogRequiredPermission",
    "CatalogResultStreamDescriptor",
    "ProcedureGatewayExecuteRequest",
    "ProcedureRouteExecuteRequest",
];
const RUNTIME_FREE_CONTRACT_TEST_FILES: [&str; 6] = [
    "tests/cancel_backpressure_contract.rs",
    "tests/connection_lifecycle_contract.rs",
    "tests/flow_control_isolation_contract.rs",
    "tests/procedure_gateway_route.rs",
    "tests/procedure_gateway_route/route_admission.rs",
    "tests/procedure_gateway_route/runtime_dispatch.rs",
];
const FORBIDDEN_RPC_RUNTIME_STACK_TOKENS: [&str; 5] =
    ["grpc", "grpcio", "prost-grpc", "prost_grpc", "tonic"];

#[derive(Debug)]
struct FeatureSpec {
    name: String,
    values: Vec<String>,
}

fn quic_crate_root() -> PathBuf {
    workspace_root().join("crates/andromeda-quic")
}

fn runtime_crate_root() -> PathBuf {
    workspace_root().join("crates/andromeda-quic-runtime-quinn")
}

fn read_quic_crate_file(relative_path: &str) -> String {
    read_file(quic_crate_root().join(relative_path))
}

fn read_runtime_crate_file(relative_path: &str) -> String {
    read_file(runtime_crate_root().join(relative_path))
}

fn read_file(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn table_lines<'a>(source: &'a str, table_header: &str) -> Vec<&'a str> {
    let mut in_table = false;
    let mut lines = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_table {
                break;
            }
            in_table = trimmed == table_header;
            continue;
        }

        if in_table {
            lines.push(line);
        }
    }

    assert!(
        in_table,
        "manifest is missing required table {table_header}"
    );
    lines
}

fn dependency_spec<'a>(dependencies: &'a [&str], dep: &str) -> Option<&'a str> {
    let prefix = format!("{dep} = ");
    let dotted_workspace = format!("{dep}.workspace = ");
    dependencies
        .iter()
        .map(|line| strip_toml_comment(line).trim())
        .find(|line| line.starts_with(&prefix) || line.starts_with(&dotted_workspace))
}

fn feature_specs(manifest: &str) -> Vec<FeatureSpec> {
    let feature_lines = table_lines(manifest, "[features]");
    let mut features = Vec::new();
    let mut index = 0;

    while index < feature_lines.len() {
        let line = strip_toml_comment(feature_lines[index]).trim();
        if line.is_empty() {
            index += 1;
            continue;
        }

        let Some((name, value_start)) = line.split_once('=') else {
            panic!("feature declaration must use key = array syntax: {line}");
        };

        let mut array_text = value_start.trim().to_string();
        while !array_text.contains(']') {
            index += 1;
            assert!(
                index < feature_lines.len(),
                "unterminated feature array for {}",
                name.trim()
            );
            array_text.push('\n');
            array_text.push_str(strip_toml_comment(feature_lines[index]).trim());
        }

        features.push(FeatureSpec {
            name: name.trim().to_string(),
            values: quoted_toml_values(&array_text),
        });
        index += 1;
    }

    features
}

fn feature_values<'a>(features: &'a [FeatureSpec], name: &str) -> &'a [String] {
    feature_values_optional(features, name)
        .unwrap_or_else(|| panic!("expected exactly one [features] entry named {name}"))
}

fn feature_values_optional<'a>(features: &'a [FeatureSpec], name: &str) -> Option<&'a [String]> {
    let matching = features
        .iter()
        .filter(|feature| feature.name == name)
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [] => None,
        [feature] => Some(feature.values.as_slice()),
        _ => panic!("expected at most one [features] entry named {name}"),
    }
}

fn quoted_toml_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in text.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                values.push(std::mem::take(&mut current));
                in_string = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_string = true;
        }
    }

    assert!(
        !in_string,
        "unterminated quoted value in TOML array: {text}"
    );
    values
}

fn strip_toml_comment(line: &str) -> &str {
    line.split_once('#')
        .map_or(line, |(before_comment, _)| before_comment)
}

fn strip_rust_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0_usize;
    let mut in_line_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            continue;
        }

        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                    output.push_str("  ");
                },
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                    output.push_str("  ");
                },
                ('\n', _) => output.push('\n'),
                _ => output.push(' '),
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('/', Some('/')) => {
                chars.next();
                in_line_comment = true;
                output.push_str("  ");
            },
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
                output.push_str("  ");
            },
            _ => output.push(ch),
        }
    }

    output
}

fn contains_forbidden_protocol_word(line: &str, word: &str) -> bool {
    line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token.eq_ignore_ascii_case(word))
}

fn production_source(content: &str) -> &str {
    content.split("#[cfg(test)]").next().unwrap_or(content)
}

#[test]
fn quic_manifest_keeps_concrete_runtime_dependencies_out() {
    let manifest = read_quic_crate_file("Cargo.toml");
    let dependencies = table_lines(&manifest, "[dependencies]");
    let features = feature_specs(&manifest);

    for dep in RUNTIME_DEPS {
        assert!(
            dependency_spec(&dependencies, dep).is_none(),
            "andromeda-quic must not own concrete runtime dependency {dep}; use {RUNTIME_CRATE}"
        );
    }

    assert!(
        feature_values(&features, "default").is_empty(),
        "andromeda-quic default features must stay empty"
    );
    assert!(
        feature_values(&features, RUNTIME_FEATURE).is_empty(),
        "{RUNTIME_FEATURE} is only a manifest marker; concrete runtime deps live in {RUNTIME_CRATE}"
    );
    assert!(
        feature_values_optional(&features, INSECURE_TEST_TLS_FEATURE).is_none(),
        "{INSECURE_TEST_TLS_FEATURE} belongs to {RUNTIME_CRATE}, not andromeda-quic"
    );
}

#[test]
fn runtime_crate_owns_quinn_rustls_tokio_dependencies() {
    let manifest = read_runtime_crate_file("Cargo.toml");
    let dependencies = table_lines(&manifest, "[dependencies]");

    for dep in RUNTIME_DEPS {
        let spec = dependency_spec(&dependencies, dep)
            .unwrap_or_else(|| panic!("{RUNTIME_CRATE} manifest is missing {dep} dependency"));
        assert!(
            spec.contains("workspace = true"),
            "{dep} should use the workspace dependency declaration: {spec}"
        );
        assert!(
            !spec.contains("optional = true"),
            "{dep} must be a direct dependency of {RUNTIME_CRATE}, not a feature hidden inside andromeda-quic: {spec}"
        );
    }

    let features = feature_specs(&manifest);
    assert!(
        feature_values(&features, "default").is_empty(),
        "{RUNTIME_CRATE} default features must stay empty"
    );
    assert_eq!(
        feature_values(&features, INSECURE_TEST_TLS_FEATURE),
        &[] as &[String],
        "{INSECURE_TEST_TLS_FEATURE} must not activate production TLS weakening"
    );
}

#[test]
fn quic_lib_has_no_concrete_runtime_modules_or_crate_paths() {
    let lib_rs = read_quic_crate_file("src/lib.rs");
    let forbidden = [
        "mod runtime_quinn;",
        "pub mod quinn_backend;",
        "pub mod quinn_tls;",
        "quinn::",
        "rustls::",
        "rcgen::",
        "tokio::",
    ];
    let violations = forbidden
        .into_iter()
        .filter(|term| lib_rs.contains(term))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-quic public surface must stay runtime-free: {violations:?}"
    );
}

#[test]
fn quic_root_does_not_reexport_gateway_projection_dtos() {
    let lib_rs = read_quic_crate_file("src/lib.rs");
    let mut violations = Vec::new();

    for token in QUIC_ROOT_FORBIDDEN_PROJECTION_REEXPORTS {
        if lib_rs.contains(token) {
            violations.push(token);
        }
    }

    assert!(
        violations.is_empty(),
        "catalog/procedure projection DTOs must be imported from andromeda-procedure-contract or andromeda-rpc-codec, not reexported by andromeda-quic: {violations:?}"
    );
}

#[test]
fn concrete_quinn_network_tests_live_in_runtime_crate() {
    let quic_manifest = read_quic_crate_file("Cargo.toml");
    for test_name in RUNTIME_QUINN_TEST_TARGETS {
        assert!(
            !quic_manifest.contains(&format!("name = \"{test_name}\"")),
            "{test_name} must not remain registered under andromeda-quic"
        );
        assert!(
            runtime_crate_root()
                .join("tests")
                .join(format!("{test_name}.rs"))
                .is_file(),
            "{test_name} must live under {RUNTIME_CRATE}/tests"
        );
    }
}

#[test]
fn drain_backpressure_and_admission_contract_tests_stay_runtime_free() {
    let mut violations = Vec::new();

    for relative_path in RUNTIME_FREE_CONTRACT_TEST_FILES {
        let source = read_quic_crate_file(relative_path);
        let code_without_comments = strip_rust_comments(&source);

        for (line_index, line) in code_without_comments.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            for dep in RUNTIME_DEPS {
                let concrete_path = format!("{dep}::");
                if compact_line.contains(&concrete_path) {
                    violations.push(format!(
                        "{relative_path}:{} exposes concrete runtime crate path {concrete_path}",
                        line_index + 1
                    ));
                }
            }
            if compact_line.contains(r#"feature="runtime-quinn""#) {
                violations.push(format!(
                    "{relative_path}:{} cfg-gates runtime-free contract coverage behind {RUNTIME_FEATURE}",
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "drain, backpressure, and admission contract coverage must remain graceful without Quinn runtime crates: {violations:?}"
    );
}

#[test]
fn runtime_manifests_stay_free_of_grpc_stack() {
    let manifests = [
        ("andromeda-quic", read_quic_crate_file("Cargo.toml")),
        (RUNTIME_CRATE, read_runtime_crate_file("Cargo.toml")),
    ];
    let mut violations = Vec::new();

    for (label, manifest) in manifests {
        for (line_index, line) in manifest.lines().enumerate() {
            let normalized = strip_toml_comment(line).trim().to_ascii_lowercase();
            if normalized.is_empty() {
                continue;
            }

            for token in FORBIDDEN_RPC_RUNTIME_STACK_TOKENS {
                if normalized.contains(token) {
                    violations.push(format!("{label}:{}: {}", line_index + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "QUIC/RPC runtime split must not add gRPC/tonic runtime stack dependencies or features: {violations:?}"
    );
}

#[test]
fn rust_protocol_surface_does_not_introduce_grpc_or_tonic() {
    let mut violations = Vec::new();
    let forbidden = ["grpc", "tonic", "prost_grpc"];

    for path in rust_source_files(&quic_crate_root().join("src"))
        .into_iter()
        .chain(rust_source_files(&runtime_crate_root().join("src")))
    {
        let content = read_file(&path);
        for (index, line) in content.lines().enumerate() {
            if forbidden
                .iter()
                .any(|word| contains_forbidden_protocol_word(line, word))
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rust protocol surface introduced a forbidden RPC stack term:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rust_protocol_surface_does_not_introduce_raw_sql_surface() {
    let mut violations = Vec::new();
    let forbidden = ["sql", "raw_sql", "execute_sql"];

    for path in rust_source_files(&quic_crate_root().join("src"))
        .into_iter()
        .chain(rust_source_files(&runtime_crate_root().join("src")))
    {
        let content = read_file(&path);
        for (index, line) in production_source(&content).lines().enumerate() {
            if forbidden
                .iter()
                .any(|word| contains_forbidden_protocol_word(line, word))
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rust protocol surface introduced a raw SQL application route:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rust_protocol_surface_does_not_introduce_runtime_json_default() {
    let manifest = quic_crate_root().join("Cargo.toml");
    let mut violations = Vec::new();

    for path in rust_source_files(&quic_crate_root().join("src"))
        .into_iter()
        .chain(rust_source_files(&runtime_crate_root().join("src")))
        .chain(std::iter::once(manifest))
    {
        let content = read_file(&path);
        for (index, line) in production_source(&content).lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if contains_forbidden_protocol_word(&lower, "json")
                || lower.contains("serde_json")
                || lower.contains("jsonrpc")
                || lower.contains("json-rpc")
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rust protocol surface introduced a runtime JSON protocol default:\n{}",
        violations.join("\n")
    );
}
