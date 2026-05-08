use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn workspace_root_from_manifest_dir(manifest_dir: &str) -> PathBuf {
    PathBuf::from(manifest_dir)
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

pub fn workspace_relative_path(workspace: &Path, path: &Path) -> String {
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

pub fn package_relative_path(package_dir: &Path, path: &Path) -> String {
    path.strip_prefix(package_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn sorted_child_paths(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

pub fn optional_sorted_child_paths(dir: &Path) -> Vec<PathBuf> {
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

pub fn unique_temp_path(prefix: &str, suffix: &str) -> PathBuf {
    let nonce = unique_nonce();
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}{suffix}", std::process::id()))
}

pub fn unique_temp_dir_path(prefix: &str) -> PathBuf {
    let nonce = unique_nonce();
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
}

pub struct TestTempDir {
    root: PathBuf,
}

impl TestTempDir {
    pub fn new(prefix: &str) -> Self {
        Self {
            root: unique_temp_dir_path(prefix),
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn display(&self) -> std::path::Display<'_> {
        self.root.display()
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos()
}
