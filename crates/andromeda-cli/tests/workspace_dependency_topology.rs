#![forbid(unsafe_code)]

#[path = "workspace_dependency_topology/diagnostics.rs"]
mod diagnostics;
#[path = "workspace_dependency_topology/forbidden_dependencies.rs"]
mod forbidden_dependencies;
#[path = "workspace_dependency_topology/graph_rules.rs"]
mod graph_rules;
#[path = "workspace_dependency_topology/manifest_loading.rs"]
mod manifest_loading;
#[path = "workspace_dependency_topology/target_crate_roadmap_rules.rs"]
mod target_crate_roadmap_rules;

use andromeda_test_support::workspace::workspace_root_from_manifest_dir;
use std::path::PathBuf;
pub(crate) fn workspace_root() -> PathBuf {
    workspace_root_from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
}
