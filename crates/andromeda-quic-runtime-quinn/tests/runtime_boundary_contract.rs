#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

const INSECURE_TEST_TLS_FEATURE: &str = "insecure-test-tls";
const RUNTIME_CRATE_FORBIDDEN_GATEWAY_PROJECTION_TOKENS: &[&str] = &[
    "andromeda_rpc_codec",
    "CatalogProcedureManifest",
    "CatalogManifestResolutionRequest",
    "CatalogManifestResolutionResponse",
    "CatalogProcedureManifestResolutionRequest",
    "CatalogProcedureManifestResolutionResponse",
    "ProcedureGatewayExecuteRequest",
    "ProcedureRouteExecuteRequest",
];

fn runtime_crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    runtime_crate_root()
        .parent()
        .expect("crate dir has workspace crates parent")
        .parent()
        .expect("workspace root exists")
        .to_path_buf()
}

fn quic_src_dir() -> PathBuf {
    workspace_root().join("crates/andromeda-quic/src")
}

fn runtime_src_dir() -> PathBuf {
    runtime_crate_root().join("src")
}

fn read_runtime_crate_file(relative_path: &str) -> String {
    read_file(runtime_crate_root().join(relative_path))
}

fn read_file(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
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

fn production_source(content: &str) -> &str {
    content.split("#[cfg(test)]").next().unwrap_or(content)
}

fn owned_runtime_sources() -> Vec<PathBuf> {
    let quic_src = quic_src_dir();
    let runtime_src = runtime_src_dir();
    let mut files = vec![
        quic_src.join("catalog_manifest_resolution.rs"),
        quic_src.join("mtls_identity.rs"),
        quic_src.join("zero_rtt.rs"),
        runtime_src.join("quinn_backend.rs"),
        runtime_src.join("quinn_tls.rs"),
        runtime_src.join("runtime_quinn.rs"),
    ];
    files.extend(rust_source_files(&quic_src.join("reconnect")));
    files.extend(rust_source_files(&runtime_src.join("quinn_backend")));
    files.sort();
    files
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

#[test]
fn runtime_crate_exposes_quinn_modules() {
    let lib_rs = read_runtime_crate_file("src/lib.rs");
    assert!(lib_rs.contains("mod runtime_quinn;"));
    assert!(lib_rs.contains("pub mod quinn_backend;"));
    assert!(lib_rs.contains("pub mod quinn_tls;"));
}

#[test]
fn concrete_quinn_runtime_stays_out_of_gateway_projection() {
    let runtime_root = runtime_crate_root();
    let mut violations = Vec::new();

    for file in rust_source_files(&runtime_root.join("src")) {
        let source = strip_rust_comments(&read_file(&file));
        let relative = relative_slash_path(&runtime_root, &file);

        for (line_index, line) in source.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            for token in RUNTIME_CRATE_FORBIDDEN_GATEWAY_PROJECTION_TOKENS {
                if compact_line.contains(token) {
                    violations.push(format!("{relative}:{} contains {token}", line_index + 1));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-quic-runtime-quinn must own concrete Quinn transport only; gateway projection belongs to procedure/rpc owner crates: {violations:?}"
    );
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
        "{INSECURE_TEST_TLS_FEATURE} must stay confined to andromeda-quic-runtime-quinn/src/quinn_tls.rs: {unexpected_refs:?}"
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
fn rustls_runtime_flags_are_hard_disabled_in_source() {
    let source = read_runtime_crate_file("src/runtime_quinn.rs");
    assert!(source.contains("config.max_early_data_size = 0;"));
    assert!(source.contains("config.send_half_rtt_data = false;"));
    assert!(source.contains("config.enable_early_data = false;"));
}

#[test]
fn doctrine_scan_rejects_unapproved_zero_rtt_enablement() {
    let mut violations = Vec::new();

    for path in rust_source_files(&quic_src_dir())
        .into_iter()
        .chain(rust_source_files(&runtime_src_dir()))
    {
        let content = read_file(&path);
        for (index, line) in content.lines().enumerate() {
            let is_enablement = line.contains("enable_early_data = true")
                || line.contains("send_half_rtt_data = true")
                || (line.contains("max_early_data_size =") && !line.contains("= 0"));
            let has_dec_marker = line.contains("DEC-");
            if is_enablement && !has_dec_marker {
                violations.push(format!("{}:{}:{}", path.display(), index + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Detected unapproved 0-RTT enablement:\n{}",
        violations.join("\n")
    );
}

#[test]
fn owned_runtime_sources_do_not_use_panic_style_escape_hatches() {
    let mut violations = Vec::new();

    for path in owned_runtime_sources() {
        let content = read_file(&path);
        for (index, line) in production_source(&content).lines().enumerate() {
            if line.contains(".unwrap(")
                || line.contains(".expect(")
                || line.contains("panic!(")
                || line.contains("unreachable!(")
                || line.contains("todo!(")
                || line.contains("unimplemented!(")
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Runtime QUIC source uses panic-style escape hatches:\n{}",
        violations.join("\n")
    );
}
