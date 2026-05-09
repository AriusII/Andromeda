use andromeda_test_support::workspace::workspace_root_from_manifest_dir;
pub(crate) use andromeda_test_support::{
    collections::{path_set, virtual_files},
    workspace::{
        optional_sorted_child_paths, package_relative_path, sorted_child_paths,
        workspace_relative_path,
    },
};
use std::path::PathBuf;

pub(crate) fn workspace_root() -> PathBuf {
    workspace_root_from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
}
