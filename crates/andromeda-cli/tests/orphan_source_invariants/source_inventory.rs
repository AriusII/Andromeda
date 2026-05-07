use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::support::{
    optional_sorted_child_paths, package_relative_path, sorted_child_paths, workspace_relative_path,
};

#[derive(Debug, Default)]
pub(crate) struct RustSourceReport {
    active: BTreeSet<PathBuf>,
    pub(crate) roots: BTreeSet<PathBuf>,
    reachable: BTreeSet<PathBuf>,
    pub(crate) module_conflicts: BTreeSet<String>,
}

impl RustSourceReport {
    pub(crate) fn orphans(&self, workspace: &Path) -> BTreeSet<String> {
        self.active
            .difference(&self.reachable)
            .map(|path| workspace_relative_path(workspace, path))
            .collect()
    }
}

pub(crate) fn collect_active_rust_sources(workspace: &Path) -> RustSourceReport {
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
    for crate_dir in sorted_child_paths(&workspace.join("crates")).expect("read crates directory") {
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
    for path in optional_sorted_child_paths(directory) {
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

pub(crate) fn collect_direct_rs_files(dir: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    for path in optional_sorted_child_paths(dir) {
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

    for path in optional_sorted_child_paths(dir) {
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

pub(crate) fn is_ignored_inventory_relative_path(path: &str) -> bool {
    path == "target"
        || path.starts_with("target/")
        || path.ends_with("/target")
        || path.contains("/target/")
        || path == ".claude/worktrees"
        || path.starts_with(".claude/worktrees/")
        || path.contains("/.claude/worktrees/")
}

pub(crate) fn compute_orphans_from_virtual_crate(
    files: &std::collections::BTreeMap<String, String>,
) -> BTreeSet<String> {
    let (reachable, _) = compute_virtual_reachability(files);
    files
        .keys()
        .filter(|path| path.ends_with(".rs") && !reachable.contains(*path))
        .cloned()
        .collect()
}

pub(crate) fn compute_module_conflicts_from_virtual_crate(
    files: &std::collections::BTreeMap<String, String>,
) -> BTreeSet<String> {
    let (_, module_conflicts) = compute_virtual_reachability(files);
    module_conflicts
}

fn compute_virtual_reachability(
    files: &std::collections::BTreeMap<String, String>,
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
    files: &std::collections::BTreeMap<String, String>,
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
