#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PRE_EXISTING_ORPHANS: &[&str] = &[
    // CLI scratch and legacy facades scheduled for cleanup.
    "crates/andromeda-cli/src/cmd_mod.rs",
    // Legacy all-in-one policy module superseded by hardware_* modules in lib.rs.
    "crates/andromeda-core/src/policy.rs",
];

const FORBIDDEN_PRODUCTION_EDGES: &[(&str, &str)] = &[
    ("andromeda-tx", "andromeda-storage"),
    ("andromeda-storage", "andromeda-exec"),
    ("andromeda-storage", "andromeda-quic"),
    ("andromeda-storage", "andromeda-srpl"),
    ("andromeda-quic", "andromeda-exec"),
];

const FORBIDDEN_CATALOG_PRODUCTION_DEPS: &[&str] = &[
    "actix",
    "actix-web",
    "async-std",
    "axum",
    "h2",
    "hyper",
    "mio",
    "quinn",
    "reqwest",
    "rustls",
    "smol",
    "tokio",
    "tokio-rustls",
    "tonic",
    "tower",
    "warp",
];

#[test]
fn orphan_guard_finds_no_new_uncompiled_duplicates_under_crates_src() {
    let workspace = workspace_root();
    let observed = collect_orphan_sources(&workspace);
    let allowed = ALLOWED_PRE_EXISTING_ORPHANS
        .iter()
        .map(|path| (*path).to_string())
        .collect::<BTreeSet<_>>();
    let new_orphans = observed.difference(&allowed).collect::<Vec<_>>();

    assert!(
        new_orphans.is_empty(),
        "new orphan Rust source files detected under crates/*/src; wire them \
         through lib.rs/main.rs or justify them in ALLOWED_PRE_EXISTING_ORPHANS:\n  - {}",
        new_orphans
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

#[test]
fn dependency_guard_enforces_workspace_doctrine() {
    let workspace = workspace_root();
    let manifests = collect_crate_manifests(&workspace);
    let mut violations = Vec::new();

    for (source, target) in FORBIDDEN_PRODUCTION_EDGES {
        if let Some(manifest) = manifests.get(*source) {
            if manifest.production_deps.contains(*target) {
                violations.push(format!(
                    "forbidden production dependency edge: {source} -> {target}"
                ));
            }
        } else {
            violations.push(format!("missing manifest for guarded crate: {source}"));
        }
    }

    if let Some(catalog) = manifests.get("andromeda-catalog") {
        for dep in &catalog.production_deps {
            if FORBIDDEN_CATALOG_PRODUCTION_DEPS.contains(&dep.as_str()) {
                violations.push(format!(
                    "catalog must not gain production network/runtime dependency: andromeda-catalog -> {dep}"
                ));
            }
        }
    } else {
        violations.push("missing manifest for guarded crate: andromeda-catalog".to_string());
    }

    for manifest in manifests.values() {
        violations.extend(manifest.forbidden_wire_deps.iter().cloned());
    }
    let workspace_manifest = fs::read_to_string(workspace.join("Cargo.toml"))
        .expect("read workspace Cargo.toml for dependency guard");
    violations.extend(
        parse_manifest("workspace", &workspace_manifest)
            .forbidden_wire_deps
            .into_iter(),
    );

    assert!(
        violations.is_empty(),
        "dependency doctrine violations detected:\n  - {}",
        violations.join("\n  - ")
    );
}

#[test]
fn orphan_guard_detects_synthetic_orphan_in_fixture() {
    let mut files = BTreeMap::new();
    files.insert(
        "src/lib.rs".to_string(),
        "mod a;\npub mod b;\npub mod inline { pub fn x() {} }\n".to_string(),
    );
    files.insert("src/a.rs".to_string(), String::new());
    files.insert("src/b/mod.rs".to_string(), "pub mod c;\n".to_string());
    files.insert("src/b/c.rs".to_string(), String::new());
    files.insert("src/orphan.rs".to_string(), String::new());
    files.insert("src/b/orphan_sub.rs".to_string(), String::new());

    let expected = ["src/b/orphan_sub.rs", "src/orphan.rs"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn dependency_guard_detects_synthetic_forbidden_edges_and_wire_deps() {
    let manifest = parse_manifest(
        "andromeda-tx",
        r#"
[dependencies]
andromeda-core.workspace = true
store = { package = "andromeda-storage", workspace = true }
serde_json = "1"

[dev-dependencies]
andromeda-storage.workspace = true

[build-dependencies]
tonic-build = "0.12"
"#,
    );

    assert!(manifest.production_deps.contains("andromeda-storage"));
    assert!(manifest.dev_deps.contains("andromeda-storage"));
    assert!(manifest
        .forbidden_wire_deps
        .iter()
        .any(|violation| violation.contains("tonic")));
    assert!(manifest
        .forbidden_wire_deps
        .iter()
        .any(|violation| violation.contains("serde_json")));
}

#[test]
fn orphan_guard_inline_mod_block_does_not_consume_a_file() {
    let mut files = BTreeMap::new();
    files.insert(
        "src/lib.rs".to_string(),
        "pub mod foo { pub fn x() {} }\n".to_string(),
    );
    files.insert("src/foo.rs".to_string(), String::new());

    let expected = ["src/foo.rs"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

fn collect_orphan_sources(workspace: &Path) -> BTreeSet<String> {
    let mut orphans = BTreeSet::new();
    for entry in fs::read_dir(workspace.join("crates")).expect("read crates directory") {
        let crate_dir = entry.expect("read crate directory entry").path();
        let src_dir = crate_dir.join("src");
        if !src_dir.is_dir() {
            continue;
        }

        let reachable = compute_reachable(&src_dir);
        for path in collect_rs_files(&src_dir) {
            if !reachable.contains(&path) {
                orphans.insert(workspace_relative_path(workspace, &path));
            }
        }
    }
    orphans
}

#[derive(Debug, Default)]
struct CargoManifest {
    production_deps: BTreeSet<String>,
    dev_deps: BTreeSet<String>,
    forbidden_wire_deps: Vec<String>,
}

fn collect_crate_manifests(workspace: &Path) -> BTreeMap<String, CargoManifest> {
    let mut manifests = BTreeMap::new();
    for entry in fs::read_dir(workspace.join("crates")).expect("read crates directory") {
        let crate_dir = entry.expect("read crate directory entry").path();
        let manifest_path = crate_dir.join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }

        let text = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", manifest_path.display()));
        let crate_name = package_name_from_manifest(&text).unwrap_or_else(|| {
            panic!(
                "manifest {} must declare [package] name",
                manifest_path.display()
            )
        });
        manifests.insert(crate_name.clone(), parse_manifest(&crate_name, &text));
    }
    manifests
}

fn package_name_from_manifest(text: &str) -> Option<String> {
    let mut section = String::new();
    for line in text.lines() {
        let trimmed = strip_toml_comment(line).trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            continue;
        }
        if section == "package" {
            if let Some((key, value)) = trimmed.split_once('=') {
                if key.trim() == "name" {
                    return Some(unquote(value.trim()).to_string());
                }
            }
        }
    }
    None
}

fn parse_manifest(crate_name: &str, text: &str) -> CargoManifest {
    let mut manifest = CargoManifest::default();
    let mut section = String::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_without_comment = strip_toml_comment(line);
        let trimmed = line_without_comment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            continue;
        }

        let is_dependency_section = section.ends_with("dependencies");
        if !is_dependency_section {
            continue;
        }

        let dependency_names = dependency_names_from_line(trimmed);
        if dependency_names.is_empty() {
            continue;
        }

        if is_production_dependency_section(&section) {
            manifest
                .production_deps
                .extend(dependency_names.iter().cloned());
            for dep in &dependency_names {
                if is_suspicious_runtime_json_dependency(dep, trimmed) {
                    manifest.forbidden_wire_deps.push(format!(
                        "{crate_name}: production dependency `{dep}` at Cargo.toml:{} is suspicious for runtime JSON wire default",
                        line_index + 1
                    ));
                }
            }
        } else if is_dev_dependency_section(&section) {
            manifest.dev_deps.extend(dependency_names.iter().cloned());
        }

        if is_forbidden_grpc_or_tonic_dependency(trimmed) {
            manifest.forbidden_wire_deps.push(format!(
                "{crate_name}: dependency line at Cargo.toml:{} mentions forbidden gRPC/tonic dependency string: {trimmed}",
                line_index + 1
            ));
        }
    }

    manifest
}

fn is_production_dependency_section(section: &str) -> bool {
    section == "dependencies"
        || (section.starts_with("target.") && section.ends_with(".dependencies"))
}

fn is_dev_dependency_section(section: &str) -> bool {
    section == "dev-dependencies"
        || (section.starts_with("target.") && section.ends_with(".dev-dependencies"))
}

fn dependency_names_from_line(line: &str) -> BTreeSet<String> {
    let Some((raw_key, value)) = line.split_once('=') else {
        return BTreeSet::new();
    };

    let mut names = BTreeSet::new();
    let key = raw_key.trim();
    let key = key.split_once('.').map(|(prefix, _)| prefix).unwrap_or(key);
    let key = unquote(key).trim();
    if !key.is_empty() {
        names.insert(key.to_string());
    }

    if let Some(package) = inline_toml_string_value(value, "package") {
        names.insert(package);
    }

    names
}

fn inline_toml_string_value(value: &str, key: &str) -> Option<String> {
    for field in value.split([',', '{', '}']) {
        let Some((candidate_key, candidate_value)) = field.split_once('=') else {
            continue;
        };
        if candidate_key.trim() != key {
            continue;
        }
        let candidate_value = candidate_value.trim();
        let quote = candidate_value.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let after_quote = &candidate_value[quote.len_utf8()..];
        let end = after_quote.find(quote)?;
        return Some(after_quote[..end].to_string());
    }
    None
}

fn is_forbidden_grpc_or_tonic_dependency(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("tonic") || lower.contains("grpc")
}

fn is_suspicious_runtime_json_dependency(dep: &str, line: &str) -> bool {
    let dep = dep.to_ascii_lowercase().replace('-', "_");
    let line = line.to_ascii_lowercase().replace('-', "_");
    dep == "serde_json" || dep == "json" || line.contains("package = \"serde_json\"")
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '#' if !in_single_quote && !in_double_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn collect_rs_files(src_dir: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    walk(src_dir, &mut |path| {
        if path.extension() == Some(OsStr::new("rs")) {
            files.insert(path.to_path_buf());
        }
    });
    files
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

fn compute_reachable(src_dir: &Path) -> BTreeSet<PathBuf> {
    let mut reachable = BTreeSet::new();
    for root in ["lib.rs", "main.rs"] {
        let path = src_dir.join(root);
        if path.is_file() {
            visit_module_file(&path, &mut reachable);
        }
    }
    reachable
}

fn visit_module_file(path: &Path, reachable: &mut BTreeSet<PathBuf>) {
    if !reachable.insert(path.to_path_buf()) {
        return;
    }

    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let child_dir = child_dir_for(path);
    for name in extract_file_mod_decls(&strip_comments(&text)) {
        let as_file = child_dir.join(format!("{name}.rs"));
        let as_mod = child_dir.join(&name).join("mod.rs");
        if as_file.is_file() {
            visit_module_file(&as_file, reachable);
        }
        if as_mod.is_file() {
            visit_module_file(&as_mod, reachable);
        }
    }
}

fn child_dir_for(path: &Path) -> PathBuf {
    let parent = path.parent().expect("module file has parent");
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
    if matches!(stem, "lib" | "main" | "mod") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    }
}

fn extract_file_mod_decls(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = strip_visibility(line.trim_start());
            let rest = line.strip_prefix("mod ")?.trim_start();
            let ident_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if ident_end == 0 {
                return None;
            }
            let (name, tail) = rest.split_at(ident_end);
            tail.trim_start().starts_with(';').then(|| name.to_string())
        })
        .collect()
}

fn strip_visibility(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let rest = rest.trim_start();
    if let Some(scoped) = rest.strip_prefix('(') {
        if let Some(close) = scoped.find(')') {
            return scoped[close + 1..].trim_start();
        }
    }
    rest
}

fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_block = false;
    while index < bytes.len() {
        if in_block {
            if index + 1 < bytes.len() && bytes[index] == b'*' && bytes[index + 1] == b'/' {
                in_block = false;
                index += 2;
            } else {
                if bytes[index] == b'\n' {
                    out.push('\n');
                }
                index += 1;
            }
            continue;
        }

        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
            in_block = true;
            index += 2;
            continue;
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn workspace_relative_path(workspace: &Path, path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    canonical
        .strip_prefix(workspace)
        .unwrap_or(&canonical)
        .to_string_lossy()
        .replace('\\', "/")
}

fn compute_orphans_from_virtual_crate(files: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut reachable = BTreeSet::new();
    for root in ["src/lib.rs", "src/main.rs"] {
        visit_virtual_module(root, files, &mut reachable);
    }
    files
        .keys()
        .filter(|path| path.ends_with(".rs") && !reachable.contains(*path))
        .cloned()
        .collect()
}

fn visit_virtual_module(
    path: &str,
    files: &BTreeMap<String, String>,
    reachable: &mut BTreeSet<String>,
) {
    let Some(text) = files.get(path) else {
        return;
    };
    if !reachable.insert(path.to_string()) {
        return;
    }

    let base = virtual_child_dir_for(path);
    for name in extract_file_mod_decls(&strip_comments(text)) {
        let as_file = virtual_join(&base, &format!("{name}.rs"));
        let as_mod = virtual_join(&virtual_join(&base, &name), "mod.rs");
        visit_virtual_module(&as_file, files, reachable);
        visit_virtual_module(&as_mod, files, reachable);
    }
}

fn virtual_child_dir_for(path: &str) -> String {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    if matches!(stem, "lib" | "main" | "mod") {
        dir.to_string()
    } else {
        virtual_join(dir, stem)
    }
}

fn virtual_join(base: &str, leaf: &str) -> String {
    if base.is_empty() {
        leaf.to_string()
    } else {
        format!("{base}/{leaf}")
    }
}
