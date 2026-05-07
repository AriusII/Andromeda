use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

pub(crate) fn workspace_relative_path(workspace: &Path, path: &Path) -> String {
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

pub(crate) fn package_relative_path(package_dir: &Path, path: &Path) -> String {
    path.strip_prefix(package_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn sorted_child_paths(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

pub(crate) fn optional_sorted_child_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

pub(crate) fn virtual_files(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
        .collect()
}

pub(crate) fn path_set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}
