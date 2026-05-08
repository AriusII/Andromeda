#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

const RUNTIME_FEATURE: &str = "runtime-quinn";
const INSECURE_TEST_TLS_FEATURE: &str = "insecure-test-tls";
const RUNTIME_CRATE: &str = "andromeda-quic-runtime-quinn";
const RUNTIME_DEPS: [&str; 4] = ["quinn", "rcgen", "rustls", "tokio"];
const RUNTIME_QUINN_TEST_TARGETS: [&str; 2] =
    ["real_quinn_network", "reconnect_quinn_admission_contract"];
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

#[test]
fn quic_manifest_keeps_concrete_runtime_dependencies_out() {
    let manifest = read_crate_file("Cargo.toml");
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
        "{RUNTIME_FEATURE} is only a compatibility selector after W22; concrete runtime deps live in {RUNTIME_CRATE}"
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
    let lib_rs = read_crate_file("src/lib.rs");
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
        "andromeda-quic public surface must stay runtime-free after W22: {violations:?}"
    );
}

#[test]
fn runtime_crate_exposes_quinn_modules() {
    let lib_rs = read_runtime_crate_file("src/lib.rs");
    assert!(lib_rs.contains("mod runtime_quinn;"));
    assert!(lib_rs.contains("pub mod quinn_backend;"));
    assert!(lib_rs.contains("pub mod quinn_tls;"));
}

#[test]
fn insecure_test_tls_is_confined_to_runtime_tls_helpers() {
    let runtime_root = runtime_crate_root();
    let mut unexpected_refs = Vec::new();
    for file in rust_source_files(&runtime_root.join("src")) {
        let source = read_file(&file);
        let relative = relative_slash_path(&runtime_root, &file);
        if relative != "src/quinn_tls.rs" && source.contains(INSECURE_TEST_TLS_FEATURE) {
            unexpected_refs.push(relative);
        }
    }

    assert!(
        unexpected_refs.is_empty(),
        "{INSECURE_TEST_TLS_FEATURE} must stay confined to {RUNTIME_CRATE}/src/quinn_tls.rs: {unexpected_refs:?}"
    );

    let quinn_tls = read_runtime_crate_file("src/quinn_tls.rs");
    assert_cfg_gated_item(
        &quinn_tls,
        "pub fn insecure_for_tests(",
        r#"#[cfg(any(test, feature = "insecure-test-tls"))]"#,
    );
    assert_cfg_gated_item(
        &quinn_tls,
        "struct InsecureVerifier;",
        r#"#[cfg(any(test, feature = "insecure-test-tls"))]"#,
    );
}

#[test]
fn concrete_quinn_network_tests_live_in_runtime_crate() {
    let quic_manifest = read_crate_file("Cargo.toml");
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
        let source = read_crate_file(relative_path);
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
        ("andromeda-quic", read_crate_file("Cargo.toml")),
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

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .expect("crate dir has workspace crates parent")
        .parent()
        .expect("workspace root exists")
        .to_path_buf()
}

fn runtime_crate_root() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("andromeda-quic-runtime-quinn")
}

fn read_crate_file(relative_path: &str) -> String {
    read_file(crate_root().join(relative_path))
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

fn assert_cfg_gated_item(source: &str, declaration: &str, expected_cfg: &str) {
    let lines = source.lines().collect::<Vec<_>>();
    let occurrences = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.trim().starts_with(declaration).then_some(index))
        .collect::<Vec<_>>();

    assert_eq!(
        occurrences.len(),
        1,
        "expected exactly one item declaration starting with `{declaration}`"
    );

    let item_line = occurrences[0];
    for line in lines[..item_line].iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[") && trimmed != expected_cfg {
            continue;
        }

        assert_eq!(
            trimmed, expected_cfg,
            "`{declaration}` must be guarded by `{expected_cfg}`"
        );
        return;
    }

    panic!("`{declaration}` has no preceding cfg guard");
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|err| {
            panic!("failed to read directory entry in {}: {err}", dir.display())
        });
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                    output.push_str("  ");
                }
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
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
                output.push_str("  ");
            }
            _ => output.push(ch),
        }
    }

    output
}
