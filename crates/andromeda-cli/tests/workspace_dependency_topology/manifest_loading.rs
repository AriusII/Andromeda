use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
pub(crate) struct CrateManifest {
    pub(crate) package_name: String,
    pub(crate) path: PathBuf,
    pub(crate) runtime_dependencies: BTreeSet<String>,
    pub(crate) dev_dependencies: BTreeSet<String>,
}
pub(crate) fn load_crate_manifests(root: &Path) -> BTreeMap<String, CrateManifest> {
    let mut manifests = BTreeMap::new();
    let workspace_aliases = load_workspace_dependency_aliases(root);

    for path in collect_manifest_paths(root) {
        let text = fs::read_to_string(&path).expect("read crate Cargo.toml");
        let package_name = parse_package_name(&text)
            .unwrap_or_else(|| panic!("{} must define package.name", path.display()));
        let dependencies = parse_dependency_names_with_aliases(&text, &workspace_aliases);

        manifests.insert(
            package_name.clone(),
            CrateManifest {
                package_name,
                path,
                runtime_dependencies: dependencies.runtime,
                dev_dependencies: dependencies.dev,
            },
        );
    }

    manifests
}
fn collect_manifest_paths(root: &Path) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    let entries = fs::read_dir(root).expect("read crates directory");

    for entry in entries {
        let entry = entry.expect("read crates directory entry");
        let path = entry.path();
        if path.is_dir() {
            let manifest = path.join("Cargo.toml");
            if manifest.exists() {
                manifests.push(manifest);
            }
        }
    }

    manifests
}
fn parse_package_name(text: &str) -> Option<String> {
    let mut in_package = false;

    for line in text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }

        if in_package && line.starts_with("name") {
            return parse_quoted_value(line);
        }
    }

    None
}
#[derive(Default)]
struct WorkspaceDependencyAliases {
    packages_by_alias: BTreeMap<String, BTreeSet<String>>,
}
impl WorkspaceDependencyAliases {
    fn from_manifest(text: &str) -> Self {
        let mut aliases = Self::default();
        let mut in_workspace_dependencies = false;

        for line in text.lines() {
            let Some(line) = cargo_line_without_comment(line) else {
                continue;
            };

            if line.starts_with('[') {
                in_workspace_dependencies = line == "[workspace.dependencies]";
                continue;
            }
            if !in_workspace_dependencies {
                continue;
            }

            let Some((raw_alias, rest)) = line.split_once('=') else {
                continue;
            };
            let alias = normalize_dependency_name(raw_alias);
            if alias.is_empty() {
                continue;
            }

            let mut names = BTreeSet::from([alias.clone()]);
            if let Some(package) = parse_package_rename(rest) {
                names.insert(normalize_dependency_name(&package));
            }
            aliases.packages_by_alias.insert(alias, names);
        }

        aliases
    }

    fn resolve(&self, alias: &str) -> BTreeSet<String> {
        self.packages_by_alias
            .get(&normalize_dependency_name(alias))
            .cloned()
            .unwrap_or_default()
    }
}
fn load_workspace_dependency_aliases(crates_root: &Path) -> WorkspaceDependencyAliases {
    let workspace_manifest = crates_root
        .parent()
        .map(|workspace| workspace.join("Cargo.toml"));
    let Some(workspace_manifest) = workspace_manifest else {
        return WorkspaceDependencyAliases::default();
    };
    let Ok(text) = fs::read_to_string(workspace_manifest) else {
        return WorkspaceDependencyAliases::default();
    };
    WorkspaceDependencyAliases::from_manifest(&text)
}
#[derive(Default)]
struct ManifestDependencies {
    runtime: BTreeSet<String>,
    dev: BTreeSet<String>,
}
fn parse_dependency_names(text: &str) -> ManifestDependencies {
    parse_dependency_names_with_aliases(text, &WorkspaceDependencyAliases::default())
}
fn parse_dependency_names_with_aliases(
    text: &str,
    workspace_aliases: &WorkspaceDependencyAliases,
) -> ManifestDependencies {
    let mut dependencies = ManifestDependencies::default();
    let mut dependency_section = ParsedDependencySection::None;

    for line in text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            dependency_section = dependency_section_for(line);
            continue;
        }

        if matches!(dependency_section, ParsedDependencySection::None) {
            continue;
        }

        if let Some((name, rest)) = line.split_once('=') {
            let dependency = normalize_dependency_name(name.trim());
            dependency_section.insert(&mut dependencies, dependency.clone());

            if dependency_uses_workspace_alias(name, rest) {
                for resolved_dependency in workspace_aliases.resolve(&dependency) {
                    dependency_section.insert(&mut dependencies, resolved_dependency);
                }
            }

            if let Some(package) = parse_package_rename(rest) {
                dependency_section.insert(&mut dependencies, normalize_dependency_name(&package));
            }
        }
    }

    dependencies
}
#[derive(Clone, Copy)]
enum ParsedDependencySection {
    None,
    Runtime,
    Dev,
}
impl ParsedDependencySection {
    fn insert(self, dependencies: &mut ManifestDependencies, dependency: String) {
        match self {
            ParsedDependencySection::None => {}
            ParsedDependencySection::Runtime => {
                dependencies.runtime.insert(dependency);
            }
            ParsedDependencySection::Dev => {
                dependencies.dev.insert(dependency);
            }
        }
    }
}
fn dependency_section_for(line: &str) -> ParsedDependencySection {
    if line == "[dependencies]"
        || line == "[build-dependencies]"
        || line.starts_with("[target.") && line.ends_with(".dependencies]")
        || line.starts_with("[target.") && line.ends_with(".build-dependencies]")
    {
        return ParsedDependencySection::Runtime;
    }

    if line == "[dev-dependencies]"
        || line.starts_with("[target.") && line.ends_with(".dev-dependencies]")
    {
        return ParsedDependencySection::Dev;
    }

    ParsedDependencySection::None
}
pub(crate) fn cargo_line_without_comment(line: &str) -> Option<&str> {
    let line = line.split('#').next().unwrap_or_default().trim();
    (!line.is_empty()).then_some(line)
}
fn dependency_uses_workspace_alias(raw_key: &str, value: &str) -> bool {
    raw_key.trim().ends_with(".workspace") || inline_toml_bool_value(value, "workspace")
}
fn inline_toml_bool_value(value: &str, key: &str) -> bool {
    for field in value.split([',', '{', '}']) {
        let Some((candidate_key, candidate_value)) = field.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key && candidate_value.trim() == "true" {
            return true;
        }
    }
    false
}
pub(crate) fn parse_package_rename(text: &str) -> Option<String> {
    text.split(',')
        .map(|part| {
            part.trim()
                .trim_matches(|character| character == '{' || character == '}')
        })
        .map(str::trim)
        .find(|part| part.starts_with("package"))
        .and_then(parse_quoted_value)
}
fn parse_quoted_value(text: &str) -> Option<String> {
    let (_, value) = text.split_once('=')?;
    let value = value.trim();
    let value = value.strip_prefix('"')?;
    let value = value.split_once('"')?.0;
    Some(value.to_owned())
}
pub(crate) fn normalize_dependency_name(name: &str) -> String {
    let name = name.trim_matches(|character: char| {
        character == '"' || character == '\'' || character.is_whitespace()
    });
    let name = name.split_once('.').map_or(name, |(key, _)| key);
    name.replace('_', "-").to_ascii_lowercase()
}
#[test]
fn manifest_parser_tracks_package_renames_in_dependency_sections() {
    let dependencies = parse_dependency_names(
        r#"
        [package]
        name = "sample"

        [dependencies]
        transport = { package = "quinn", workspace = true }
        ignored = { package = "andromeda-exec", workspace = true }

        [dev-dependencies]
        temp = { package = "tempfile", workspace = true }
        proptest.workspace = true

        [build-dependencies]
        proto = { package = "prost-build", workspace = true }

        [workspace.dependencies]
        andromeda-bench = { path = "crates/andromeda-bench" }
        "#,
    );

    assert!(dependencies.runtime.contains("quinn"));
    assert!(dependencies.runtime.contains("andromeda-exec"));
    assert!(dependencies.runtime.contains("prost-build"));
    assert!(dependencies.dev.contains("tempfile"));
    assert!(dependencies.dev.contains("proptest"));
    assert!(!dependencies.runtime.contains("andromeda-bench"));
    assert!(!dependencies.dev.contains("andromeda-bench"));
}
#[test]
fn manifest_parser_resolves_workspace_dependency_aliases_before_rules() {
    let workspace_aliases = WorkspaceDependencyAliases::from_manifest(
        r#"
        [workspace.dependencies]
        transport = { package = "quinn", version = "0.11" }
        grpc_wire = { package = "tonic", version = "0.12" }
        json_wire = { package = "serde_json", version = "1" }
        sql_backend = { package = "sqlx", version = "0.8" }
        native_layout = { package = "bytemuck", version = "1" }
        "#,
    );
    let dependencies = parse_dependency_names_with_aliases(
        r#"
        [dependencies]
        transport.workspace = true
        grpc_wire.workspace = true
        json_wire.workspace = true
        sql_backend = { workspace = true }

        [build-dependencies]
        native_layout.workspace = true
        "#,
        &workspace_aliases,
    );

    assert!(dependencies.runtime.contains("quinn"));
    assert!(dependencies.runtime.contains("tonic"));
    assert!(dependencies.runtime.contains("serde-json"));
    assert!(dependencies.runtime.contains("sqlx"));
    assert!(dependencies.runtime.contains("bytemuck"));
}
