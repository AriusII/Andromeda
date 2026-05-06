#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PRE_EXISTING_ORPHANS: &[&str] = &[];

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
fn orphan_guard_finds_no_new_active_rust_orphans_or_module_conflicts() {
    let workspace = workspace_root();
    let report = collect_active_rust_sources(&workspace);
    let observed = report.orphans(&workspace);
    let allowed = ALLOWED_PRE_EXISTING_ORPHANS
        .iter()
        .map(|path| (*path).to_string())
        .collect::<BTreeSet<_>>();
    let new_orphans = observed.difference(&allowed).collect::<Vec<_>>();

    assert!(
        new_orphans.is_empty() && report.module_conflicts.is_empty(),
        "active Rust source inventory violations detected.\n\
         New orphan files must be wired through a Cargo root, mod/#[path], or include!, \
         or justified in ALLOWED_PRE_EXISTING_ORPHANS. Module conflicts must remove either \
         foo.rs or foo/mod.rs for the same declared module.\n\
         \nNew orphans:\n  - {}\n\nModule file conflicts:\n  - {}",
        new_orphans
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n  - "),
        report
            .module_conflicts
            .iter()
            .map(|conflict| conflict.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

#[test]
fn active_rust_inventory_accounts_for_build_scripts_fuzz_targets_and_exclusions() {
    let workspace = workspace_root();
    let report = collect_active_rust_sources(&workspace);
    let roots = report
        .roots
        .iter()
        .map(|path| workspace_relative_path(&workspace, path))
        .collect::<BTreeSet<_>>();

    assert!(
        roots.contains("crates/andromeda-proto/build.rs"),
        "active inventory must include crate build scripts"
    );
    for fuzz_target in collect_direct_rs_files(&workspace.join("fuzz").join("fuzz_targets")) {
        let fuzz_target = workspace_relative_path(&workspace, &fuzz_target);
        assert!(
            roots.contains(&fuzz_target),
            "active inventory must include fuzz target root {fuzz_target}"
        );
    }
    assert!(
        roots
            .iter()
            .all(|path| !is_ignored_inventory_relative_path(path)),
        "active inventory roots must exclude target, fuzz/target, and .claude/worktrees paths"
    );

    for ignored in [
        "target/generated.rs",
        "fuzz/target/generated.rs",
        ".claude/worktrees/wave/src/lib.rs",
        "crates/andromeda-cli/target/debug/build.rs",
    ] {
        assert!(
            is_ignored_inventory_relative_path(ignored),
            "inventory exclusion should ignore {ignored}"
        );
    }
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
    violations.extend(parse_manifest("workspace", &workspace_manifest).forbidden_wire_deps);

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
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("tonic"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("serde_json"))
    );
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

#[test]
fn orphan_guard_file_module_reaches_sibling_directory_modules() {
    let mut files = BTreeMap::new();
    files.insert("src/lib.rs".to_string(), "mod file;\n".to_string());
    files.insert("src/file.rs".to_string(), "mod child;\n".to_string());
    files.insert("src/file/child.rs".to_string(), String::new());
    files.insert("src/file/orphan.rs".to_string(), String::new());

    let expected = ["src/file/orphan.rs"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn orphan_guard_include_macro_counts_literal_rust_source() {
    let mut files = BTreeMap::new();
    files.insert(
        "src/lib.rs".to_string(),
        "include!(\"generated/included.rs\");\n".to_string(),
    );
    files.insert("src/generated/included.rs".to_string(), String::new());

    assert!(compute_orphans_from_virtual_crate(&files).is_empty());
}

#[test]
fn orphan_guard_detects_synthetic_file_vs_mod_rs_conflict() {
    let mut files = BTreeMap::new();
    files.insert("src/lib.rs".to_string(), "mod optimizer;\n".to_string());
    files.insert("src/optimizer.rs".to_string(), String::new());
    files.insert("src/optimizer/mod.rs".to_string(), String::new());

    let expected =
        ["src/lib.rs: mod optimizer resolves to both src/optimizer.rs and src/optimizer/mod.rs"]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>();

    assert_eq!(
        compute_module_conflicts_from_virtual_crate(&files),
        expected
    );
}

#[test]
fn orphan_guard_path_attribute_resolves_from_declaring_file_directory() {
    let mut files = BTreeMap::new();
    files.insert("src/lib.rs".to_string(), "mod manager;\n".to_string());
    files.insert(
        "src/manager.rs".to_string(),
        "mod manager_core;\n".to_string(),
    );
    files.insert(
        "src/manager/manager_core.rs".to_string(),
        "#[cfg(test)]\n#[path = \"tests.rs\"]\nmod tests;\n".to_string(),
    );
    files.insert("src/manager/tests.rs".to_string(), String::new());
    files.insert(
        "src/manager/manager_core/tests.rs".to_string(),
        String::new(),
    );

    let expected = ["src/manager/manager_core/tests.rs"]
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

#[derive(Debug, Default)]
struct RustSourceReport {
    active: BTreeSet<PathBuf>,
    roots: BTreeSet<PathBuf>,
    reachable: BTreeSet<PathBuf>,
    module_conflicts: BTreeSet<String>,
}

impl RustSourceReport {
    fn orphans(&self, workspace: &Path) -> BTreeSet<String> {
        self.active
            .difference(&self.reachable)
            .map(|path| workspace_relative_path(workspace, path))
            .collect()
    }
}

fn collect_active_rust_sources(workspace: &Path) -> RustSourceReport {
    let mut report = RustSourceReport::default();

    for package_dir in collect_rust_package_dirs(workspace) {
        report
            .active
            .extend(collect_active_rs_files(workspace, &package_dir));

        let roots = collect_cargo_target_roots(&package_dir);
        report.roots.extend(roots.iter().cloned());
        for root in roots {
            visit_module_file(&root, &mut report.reachable, &mut report.module_conflicts);
        }
    }

    report
}

fn collect_rust_package_dirs(workspace: &Path) -> Vec<PathBuf> {
    let mut package_dirs = Vec::new();
    for entry in fs::read_dir(workspace.join("crates")).expect("read crates directory") {
        let crate_dir = entry.expect("read crate directory entry").path();
        if crate_dir.join("Cargo.toml").is_file() {
            package_dirs.push(crate_dir);
        }
    }

    let fuzz_dir = workspace.join("fuzz");
    if fuzz_dir.join("Cargo.toml").is_file() {
        package_dirs.push(fuzz_dir);
    }

    package_dirs
}

fn collect_active_rs_files(workspace: &Path, package_dir: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    walk_inventory(package_dir, workspace, &mut |path| {
        if path.extension() == Some(OsStr::new("rs"))
            && is_active_rust_source_candidate(package_dir, path)
        {
            files.insert(path.to_path_buf());
        }
    });
    files
}

fn is_active_rust_source_candidate(package_dir: &Path, path: &Path) -> bool {
    let relative = package_relative_path(package_dir, path);
    relative == "build.rs"
        || relative.starts_with("src/")
        || relative.starts_with("tests/")
        || relative.starts_with("benches/")
        || relative.starts_with("examples/")
        || relative.starts_with("fuzz_targets/")
}

fn collect_cargo_target_roots(package_dir: &Path) -> BTreeSet<PathBuf> {
    let manifest_path = package_dir.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", manifest_path.display()));
    let declared = parse_manifest_target_paths(&manifest);
    let mut roots = BTreeSet::new();

    match declared.build_script {
        Some(Some(path)) => add_if_file(&mut roots, package_dir.join(path)),
        Some(None) => {}
        None => add_if_file(&mut roots, package_dir.join("build.rs")),
    }
    for path in declared.explicit_source_paths {
        add_if_file(&mut roots, package_dir.join(path));
    }

    add_if_file(&mut roots, package_dir.join("src").join("lib.rs"));
    add_if_file(&mut roots, package_dir.join("src").join("main.rs"));
    add_cargo_convention_roots(&mut roots, &package_dir.join("src").join("bin"));
    add_cargo_convention_roots(&mut roots, &package_dir.join("tests"));
    add_cargo_convention_roots(&mut roots, &package_dir.join("benches"));
    add_cargo_convention_roots(&mut roots, &package_dir.join("examples"));
    add_cargo_convention_roots(&mut roots, &package_dir.join("fuzz_targets"));

    roots
}

#[derive(Debug, Default)]
struct ManifestTargetPaths {
    explicit_source_paths: BTreeSet<String>,
    build_script: Option<Option<String>>,
}

fn parse_manifest_target_paths(text: &str) -> ManifestTargetPaths {
    let mut paths = ManifestTargetPaths::default();
    let mut section = String::new();

    for line in text.lines() {
        let trimmed = strip_toml_comment(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        if section == "package" && key == "build" {
            paths.build_script = if value == "false" {
                Some(None)
            } else {
                Some(Some(unquote(value).replace('\\', "/")))
            };
        } else if key == "path"
            && matches!(
                section.as_str(),
                "lib" | "bin" | "test" | "bench" | "example"
            )
        {
            paths
                .explicit_source_paths
                .insert(unquote(value).replace('\\', "/"));
        }
    }

    paths
}

fn add_cargo_convention_roots(roots: &mut BTreeSet<PathBuf>, directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            add_if_file(roots, path.join("main.rs"));
        } else if path.extension() == Some(OsStr::new("rs")) {
            roots.insert(path);
        }
    }
}

fn add_if_file(files: &mut BTreeSet<PathBuf>, path: PathBuf) {
    if path.is_file() {
        files.insert(path);
    }
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
        if section != "package" {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() == "name" {
            return Some(unquote(value.trim()).to_string());
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

fn collect_direct_rs_files(dir: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("rs")) {
            files.insert(path);
        }
    }
    files
}

fn walk_inventory(dir: &Path, workspace: &Path, visit: &mut dyn FnMut(&Path)) {
    if is_ignored_inventory_path(workspace, dir) {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            walk_inventory(&path, workspace, visit);
        } else if !is_ignored_inventory_path(workspace, &path) {
            visit(&path);
        }
    }
}

fn visit_module_file(
    path: &Path,
    reachable: &mut BTreeSet<PathBuf>,
    module_conflicts: &mut BTreeSet<String>,
) {
    if !reachable.insert(path.to_path_buf()) {
        return;
    }

    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let active_source = strip_comments(&text);
    let parent = path.parent().expect("module file has parent");
    for include_path in extract_include_paths(&active_source) {
        let explicit_path = parent.join(include_path);
        if explicit_path.is_file() {
            visit_module_file(&explicit_path, reachable, module_conflicts);
        }
    }

    let child_dir = child_dir_for(path);
    for decl in extract_file_mod_decls(&active_source) {
        if let Some(path_override) = decl.path_override {
            let explicit_path = parent.join(path_override);
            if explicit_path.is_file() {
                visit_module_file(&explicit_path, reachable, module_conflicts);
            }
        } else {
            let as_file = child_dir.join(format!("{}.rs", decl.name));
            let as_mod = child_dir.join(&decl.name).join("mod.rs");
            if as_file.is_file() && as_mod.is_file() {
                module_conflicts.insert(format!(
                    "{}: mod {} resolves to both {} and {}",
                    path.display(),
                    decl.name,
                    as_file.display(),
                    as_mod.display()
                ));
            }
            if as_file.is_file() {
                visit_module_file(&as_file, reachable, module_conflicts);
            }
            if as_mod.is_file() {
                visit_module_file(&as_mod, reachable, module_conflicts);
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleDecl {
    name: String,
    path_override: Option<String>,
}

fn extract_file_mod_decls(source: &str) -> Vec<ModuleDecl> {
    let mut decls = Vec::new();
    let mut pending_path_override = None;

    for raw_line in source.lines() {
        let line = raw_line.trim_start();
        if let Some(path_override) = parse_path_attribute(line) {
            pending_path_override = Some(path_override);
            continue;
        }
        if line.starts_with("#[") || line.is_empty() {
            continue;
        }

        let line = strip_visibility(line);
        let Some(rest) = line.strip_prefix("mod ").map(str::trim_start) else {
            pending_path_override = None;
            continue;
        };
        let ident_end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if ident_end == 0 {
            pending_path_override = None;
            continue;
        }
        let (name, tail) = rest.split_at(ident_end);
        if tail.trim_start().starts_with(';') {
            decls.push(ModuleDecl {
                name: name.to_string(),
                path_override: pending_path_override.take(),
            });
        } else {
            pending_path_override = None;
        }
    }

    decls
}

fn parse_path_attribute(line: &str) -> Option<String> {
    let rest = line.strip_prefix("#[")?.trim_start();
    let rest = rest.strip_prefix("path")?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].replace('\\', "/"))
}

fn extract_include_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let Some(index) = line.find("include!") else {
            continue;
        };
        let rest = line[index + "include!".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('(').map(str::trim_start) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        paths.push(rest[..end].replace('\\', "/"));
    }
    paths
}

fn strip_visibility(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let rest = rest.trim_start();
    if let Some((scoped, close)) = rest
        .strip_prefix('(')
        .and_then(|scoped| scoped.find(')').map(|close| (scoped, close)))
    {
        return scoped[close + 1..].trim_start();
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

fn is_ignored_inventory_path(workspace: &Path, path: &Path) -> bool {
    is_ignored_inventory_relative_path(&workspace_relative_path(workspace, path))
}

fn is_ignored_inventory_relative_path(path: &str) -> bool {
    path == "target"
        || path.starts_with("target/")
        || path.ends_with("/target")
        || path.contains("/target/")
        || path == ".claude/worktrees"
        || path.starts_with(".claude/worktrees/")
        || path.contains("/.claude/worktrees/")
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

fn package_relative_path(package_dir: &Path, path: &Path) -> String {
    path.strip_prefix(package_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn compute_orphans_from_virtual_crate(files: &BTreeMap<String, String>) -> BTreeSet<String> {
    let (reachable, _) = compute_virtual_reachability(files);
    files
        .keys()
        .filter(|path| path.ends_with(".rs") && !reachable.contains(*path))
        .cloned()
        .collect()
}

fn compute_module_conflicts_from_virtual_crate(
    files: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let (_, module_conflicts) = compute_virtual_reachability(files);
    module_conflicts
}

fn compute_virtual_reachability(
    files: &BTreeMap<String, String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut reachable = BTreeSet::new();
    let mut module_conflicts = BTreeSet::new();
    for root in ["src/lib.rs", "src/main.rs"] {
        visit_virtual_module(root, files, &mut reachable, &mut module_conflicts);
    }
    (reachable, module_conflicts)
}

fn visit_virtual_module(
    path: &str,
    files: &BTreeMap<String, String>,
    reachable: &mut BTreeSet<String>,
    module_conflicts: &mut BTreeSet<String>,
) {
    let Some(text) = files.get(path) else {
        return;
    };
    if !reachable.insert(path.to_string()) {
        return;
    }

    let base = virtual_child_dir_for(path);
    let parent = virtual_parent_dir_for(path);
    let active_source = strip_comments(text);
    for include_path in extract_include_paths(&active_source) {
        let explicit_path = virtual_join(&parent, &include_path);
        visit_virtual_module(&explicit_path, files, reachable, module_conflicts);
    }
    for decl in extract_file_mod_decls(&active_source) {
        if let Some(path_override) = decl.path_override {
            let explicit_path = virtual_join(&parent, &path_override);
            visit_virtual_module(&explicit_path, files, reachable, module_conflicts);
        } else {
            let as_file = virtual_join(&base, &format!("{}.rs", decl.name));
            let as_mod = virtual_join(&virtual_join(&base, &decl.name), "mod.rs");
            if files.contains_key(&as_file) && files.contains_key(&as_mod) {
                module_conflicts.insert(format!(
                    "{path}: mod {} resolves to both {as_file} and {as_mod}",
                    decl.name
                ));
            }
            visit_virtual_module(&as_file, files, reachable, module_conflicts);
            visit_virtual_module(&as_mod, files, reachable, module_conflicts);
        }
    }
}

fn virtual_parent_dir_for(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
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
