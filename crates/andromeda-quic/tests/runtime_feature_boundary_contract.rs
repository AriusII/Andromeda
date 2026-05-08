#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

const RUNTIME_FEATURE: &str = "runtime-quinn";
const INSECURE_TEST_TLS_FEATURE: &str = "insecure-test-tls";
const DEFERRED_RUNTIME_CRATE: &str = "andromeda-runtime-quinn";
const RUNTIME_DEPS: [&str; 4] = ["quinn", "rcgen", "rustls", "tokio"];
const TOKIO_RUNTIME_FEATURES: [&str; 3] =
    ["tokio/io-util", "tokio/macros", "tokio/rt-multi-thread"];
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
fn concrete_runtime_dependencies_are_optional_and_runtime_quinn_owned() {
    let manifest = read_crate_file("Cargo.toml");
    let dependencies = table_lines(&manifest, "[dependencies]");
    let features = feature_specs(&manifest);

    for dep in RUNTIME_DEPS {
        let spec = dependency_spec(&dependencies, dep);
        assert!(
            spec.contains("optional = true"),
            "{dep} must remain optional so default builds stay runtime-free: {spec}"
        );
        assert!(
            spec.contains("workspace = true"),
            "{dep} should keep using the workspace dependency declaration: {spec}"
        );
    }

    assert!(
        feature_values(&features, "default").is_empty(),
        "andromeda-quic default features must stay empty"
    );
    assert_deferred_runtime_crate_is_not_wired(&dependencies, &features);

    let runtime_values = feature_values(&features, RUNTIME_FEATURE);
    for dep in RUNTIME_DEPS {
        let dep_activation = format!("dep:{dep}");
        assert!(
            runtime_values.contains(&dep_activation),
            "{RUNTIME_FEATURE} must explicitly own {dep_activation}"
        );
    }
    for tokio_feature in TOKIO_RUNTIME_FEATURES {
        assert!(
            runtime_values.iter().any(|value| value == tokio_feature),
            "{RUNTIME_FEATURE} must own Tokio runtime subfeature {tokio_feature}"
        );
    }

    let direct_runtime_leaks = features
        .iter()
        .filter(|feature| feature.name != RUNTIME_FEATURE)
        .flat_map(non_runtime_feature_runtime_dependency_leaks)
        .collect::<Vec<_>>();

    assert!(
        direct_runtime_leaks.is_empty(),
        "only {RUNTIME_FEATURE} may directly activate concrete runtime dependencies: {direct_runtime_leaks:?}"
    );
}

#[test]
fn insecure_test_tls_is_test_only_and_activates_runtime_quinn() {
    let manifest = read_crate_file("Cargo.toml");
    let features = feature_specs(&manifest);

    assert!(
        feature_values(&features, "default")
            .iter()
            .all(|value| value != INSECURE_TEST_TLS_FEATURE),
        "{INSECURE_TEST_TLS_FEATURE} must not be part of default features"
    );

    let expected_insecure_values = [RUNTIME_FEATURE.to_string()];
    assert_eq!(
        feature_values(&features, INSECURE_TEST_TLS_FEATURE),
        expected_insecure_values.as_slice(),
        "{INSECURE_TEST_TLS_FEATURE} must only activate {RUNTIME_FEATURE}; it must not directly weaken production TLS or pull concrete runtime deps"
    );

    let crate_root = crate_root();
    let mut unexpected_refs = Vec::new();
    for file in rust_source_files(&crate_root.join("src")) {
        let source = read_file(&file);
        let relative = relative_slash_path(&crate_root, &file);
        if relative != "src/quinn_tls.rs" && source.contains(INSECURE_TEST_TLS_FEATURE) {
            unexpected_refs.push(relative);
        }
    }
    assert!(
        unexpected_refs.is_empty(),
        "{INSECURE_TEST_TLS_FEATURE} must stay confined to src/quinn_tls.rs test helpers: {unexpected_refs:?}"
    );

    let quinn_tls = read_crate_file("src/quinn_tls.rs");
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
fn runtime_quinn_integration_tests_require_runtime_feature_targets() {
    let manifest = read_crate_file("Cargo.toml");

    for test_name in RUNTIME_QUINN_TEST_TARGETS {
        let target = test_target_block(&manifest, test_name);
        let expected_path = format!("path = \"tests/{test_name}.rs\"");
        assert!(
            target.lines().any(|line| line.trim() == expected_path),
            "{test_name} must stay bound to tests/{test_name}.rs"
        );
        assert!(
            target
                .lines()
                .any(|line| line.trim() == r#"required-features = ["runtime-quinn"]"#),
            "{test_name} must not compile as an empty test target when {RUNTIME_FEATURE} is omitted"
        );
    }
}

#[test]
fn only_concrete_runtime_tests_require_runtime_quinn_feature() {
    let manifest = read_crate_file("Cargo.toml");
    let mut unexpected_runtime_gates = Vec::new();

    for block in manifest.split("[[test]]").skip(1) {
        let requires_runtime_quinn = block
            .lines()
            .map(|line| strip_toml_comment(line).trim())
            .any(|line| {
                line.starts_with("required-features")
                    && quoted_toml_values(line)
                        .iter()
                        .any(|value| value == RUNTIME_FEATURE)
            });
        if !requires_runtime_quinn {
            continue;
        }

        let is_expected_runtime_target = RUNTIME_QUINN_TEST_TARGETS.iter().any(|test_name| {
            block
                .lines()
                .any(|line| line.trim() == format!("name = \"{test_name}\""))
        });
        if !is_expected_runtime_target {
            unexpected_runtime_gates.push(block.lines().take(4).collect::<Vec<_>>().join(" | "));
        }
    }

    assert!(
        unexpected_runtime_gates.is_empty(),
        "drain, backpressure, and route-admission contract tests must stay runtime-free; only \
         concrete Quinn network targets may require {RUNTIME_FEATURE}: {unexpected_runtime_gates:?}"
    );
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
        "drain, backpressure, and admission contract coverage must remain graceful without the \
         concrete Quinn runtime feature: {violations:?}"
    );
}

#[test]
fn cargo_manifest_keeps_quic_runtime_boundary_free_of_grpc_stack() {
    let manifest = read_crate_file("Cargo.toml");
    let mut violations = Vec::new();

    for (line_index, line) in manifest.lines().enumerate() {
        let normalized = strip_toml_comment(line).trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }

        for token in FORBIDDEN_RPC_RUNTIME_STACK_TOKENS {
            if normalized.contains(token) {
                violations.push(format!("{}: {}", line_index + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-quic must not add gRPC/tonic runtime stack dependencies or features: {violations:?}"
    );
}
#[test]
fn lib_rs_cfg_gates_concrete_runtime_modules() {
    let lib_rs = read_crate_file("src/lib.rs");

    assert_cfg_gated_item(
        &lib_rs,
        "mod runtime_quinn;",
        r#"#[cfg(feature = "runtime-quinn")]"#,
    );
    assert_cfg_gated_item(
        &lib_rs,
        "pub mod quinn_backend;",
        r#"#[cfg(feature = "runtime-quinn")]"#,
    );
    assert_cfg_gated_item(
        &lib_rs,
        "pub mod quinn_tls;",
        r#"#[cfg(feature = "runtime-quinn")]"#,
    );
}

#[test]
fn non_runtime_source_surface_has_no_concrete_runtime_crate_paths() {
    let crate_root = crate_root();
    let mut violations = Vec::new();

    for file in rust_source_files(&crate_root.join("src")) {
        if is_runtime_owned_source(&crate_root, &file) {
            continue;
        }

        let source = read_file(&file);
        let code_without_comments = strip_rust_comments(&source);
        let relative = relative_slash_path(&crate_root, &file);

        for (line_index, line) in code_without_comments.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            for dep in RUNTIME_DEPS {
                let concrete_path = format!("{dep}::");
                if compact_line.contains(&concrete_path) {
                    violations.push(format!(
                        "{}:{} exposes concrete runtime crate path {concrete_path}",
                        relative,
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "default non-runtime source files must not expose quinn/rustls/tokio/rcgen paths: {violations:?}"
    );
}

fn assert_deferred_runtime_crate_is_not_wired(dependencies: &[&str], features: &[FeatureSpec]) {
    let dependency_key = format!("{DEFERRED_RUNTIME_CRATE} = ");
    let package_value = format!("package = \"{DEFERRED_RUNTIME_CRATE}\"");
    let manifest_edges = dependencies
        .iter()
        .map(|line| strip_toml_comment(line).trim())
        .filter(|line| line.starts_with(&dependency_key) || line.contains(&package_value))
        .collect::<Vec<_>>();

    assert!(
        manifest_edges.is_empty(),
        "{DEFERRED_RUNTIME_CRATE} extraction is deferred; andromeda-quic must keep owning concrete runtime dependencies directly for now: {manifest_edges:?}"
    );

    let feature_edges = features
        .iter()
        .flat_map(|feature| {
            feature
                .values
                .iter()
                .filter(|value| value.contains(DEFERRED_RUNTIME_CRATE))
                .map(|value| format!("{} -> {}", feature.name, value))
        })
        .collect::<Vec<_>>();

    assert!(
        feature_edges.is_empty(),
        "{DEFERRED_RUNTIME_CRATE} extraction is deferred; no feature may activate it yet: {feature_edges:?}"
    );
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_crate_file(relative_path: &str) -> String {
    read_file(crate_root().join(relative_path))
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

fn test_target_block<'a>(manifest: &'a str, test_name: &str) -> &'a str {
    manifest
        .split("[[test]]")
        .skip(1)
        .find(|block| {
            block
                .lines()
                .any(|line| line.trim() == format!("name = \"{test_name}\""))
        })
        .unwrap_or_else(|| panic!("manifest is missing [[test]] target for {test_name}"))
}
fn dependency_spec<'a>(dependencies: &'a [&str], dep: &str) -> &'a str {
    let prefix = format!("{dep} = ");
    dependencies
        .iter()
        .map(|line| strip_toml_comment(line).trim())
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("manifest is missing [dependencies] entry for {dep}"))
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
    let matching = features
        .iter()
        .filter(|feature| feature.name == name)
        .collect::<Vec<_>>();

    assert_eq!(
        matching.len(),
        1,
        "expected exactly one [features] entry named {name}"
    );

    matching[0].values.as_slice()
}

fn non_runtime_feature_runtime_dependency_leaks(feature: &FeatureSpec) -> Vec<String> {
    let mut leaks = Vec::new();

    for value in &feature.values {
        for dep in RUNTIME_DEPS {
            let dep_activation = format!("dep:{dep}");
            let dep_subfeature_prefix = format!("{dep}/");
            if value == &dep_activation || value.starts_with(&dep_subfeature_prefix) {
                leaks.push(format!("{} -> {}", feature.name, value));
            }
        }
    }

    leaks
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

fn is_runtime_owned_source(crate_root: &Path, path: &Path) -> bool {
    let relative = relative_slash_path(crate_root, path);
    matches!(
        relative.as_str(),
        "src/runtime_quinn.rs" | "src/quinn_backend.rs" | "src/quinn_tls.rs"
    ) || relative.starts_with("src/quinn_backend/")
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
