use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::support::sorted_child_paths;

const FORBIDDEN_GENERIC_CRATE_NAME_PARTS: &[&str] = &["common", "utils", "misc", "helpers"];

#[derive(Debug, Default)]
pub(crate) struct DependencyManifest {
    pub(crate) production_deps: BTreeSet<String>,
    pub(crate) dev_deps: BTreeSet<String>,
    pub(crate) forbidden_wire_deps: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct WorkspaceDependencyAliases {
    packages_by_alias: BTreeMap<String, BTreeSet<String>>,
}

impl WorkspaceDependencyAliases {
    pub(crate) fn from_manifest(text: &str) -> Self {
        let mut aliases = Self::default();
        let mut section = String::new();

        for line in text.lines() {
            let trimmed = strip_toml_comment(line).trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed.trim_matches(&['[', ']'][..]).to_string();
                continue;
            }
            if section != "workspace.dependencies" {
                continue;
            }

            let Some((raw_key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let alias = normalize_dependency_name(unquote(raw_key.trim()));
            if alias.is_empty() {
                continue;
            }

            let mut names = BTreeSet::new();
            insert_dependency_name(&mut names, &alias);
            if let Some(package) = inline_toml_string_value(value, "package") {
                insert_dependency_name(&mut names, &package);
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

pub(crate) fn crate_name_uses_generic_topology_bucket(name: &str) -> Option<&'static str> {
    let normalized = normalize_dependency_name(name);
    let ownership_name = normalized
        .strip_prefix("andromeda-")
        .unwrap_or(normalized.as_str());

    for &bucket in FORBIDDEN_GENERIC_CRATE_NAME_PARTS {
        if ownership_name.split('-').any(|part| part == bucket) {
            return Some(bucket);
        }
    }

    if ownership_name.contains("god-engine") || ownership_name.contains("godengine") {
        return Some("god_engine");
    }

    None
}

pub(crate) fn collect_dependency_manifests(
    workspace: &Path,
) -> BTreeMap<String, DependencyManifest> {
    let mut manifests = BTreeMap::new();
    let workspace_manifest = fs::read_to_string(workspace.join("Cargo.toml"))
        .expect("read workspace Cargo.toml for dependency aliases");
    let workspace_aliases = WorkspaceDependencyAliases::from_manifest(&workspace_manifest);

    for crate_dir in sorted_child_paths(&workspace.join("crates")).expect("read crates directory") {
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
        manifests.insert(
            crate_name.clone(),
            parse_dependency_manifest_with_aliases(&crate_name, &text, &workspace_aliases),
        );
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

pub(crate) fn parse_dependency_manifest(crate_name: &str, text: &str) -> DependencyManifest {
    parse_dependency_manifest_with_aliases(crate_name, text, &WorkspaceDependencyAliases::default())
}

pub(crate) fn parse_dependency_manifest_with_aliases(
    crate_name: &str,
    text: &str,
    workspace_aliases: &WorkspaceDependencyAliases,
) -> DependencyManifest {
    let mut manifest = DependencyManifest::default();
    let mut dependency_section = None;

    for (line_index, line) in text.lines().enumerate() {
        let line_without_comment = strip_toml_comment(line);
        let trimmed = line_without_comment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            dependency_section = parse_dependency_section(&section);
            continue;
        }

        let Some(dependency_section) = &dependency_section else {
            continue;
        };

        let dependency_names = if let Some(section_dependency_name) = &dependency_section.name {
            dependency_names_from_table_line(trimmed, section_dependency_name, workspace_aliases)
        } else {
            dependency_names_from_line(trimmed, workspace_aliases)
        };
        if dependency_names.is_empty() {
            continue;
        }

        if dependency_section.kind == DependencySectionKind::Production {
            manifest
                .production_deps
                .extend(dependency_names.iter().cloned());
            for dep in &dependency_names {
                if let Some(reason) = forbidden_production_dependency_reason(dep, trimmed) {
                    manifest.forbidden_wire_deps.push(format!(
                        "{crate_name}: production dependency `{dep}` at Cargo.toml:{} is forbidden for {reason}",
                        line_index + 1
                    ));
                }
            }
        } else if dependency_section.kind == DependencySectionKind::Dev {
            manifest.dev_deps.extend(dependency_names.iter().cloned());
        }

        if is_forbidden_grpc_or_tonic_dependency_name(trimmed)
            || dependency_names
                .iter()
                .any(|dep| is_forbidden_grpc_or_tonic_dependency_name(dep))
        {
            let dependency_names = dependency_names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            manifest.forbidden_wire_deps.push(format!(
                "{crate_name}: dependency line at Cargo.toml:{} mentions forbidden gRPC/tonic dependency string; dependency names: {dependency_names}; line: {trimmed}",
                line_index + 1
            ));
        }
    }

    manifest
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DependencySectionKind {
    Production,
    Dev,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencySection {
    kind: DependencySectionKind,
    name: Option<String>,
}

fn parse_dependency_section(section: &str) -> Option<DependencySection> {
    [
        ("dependencies", DependencySectionKind::Production),
        ("dev-dependencies", DependencySectionKind::Dev),
        ("build-dependencies", DependencySectionKind::Build),
        ("workspace.dependencies", DependencySectionKind::Production),
    ]
    .into_iter()
    .find_map(|(section_name, kind)| {
        dependency_name_for_section(section, section_name)
            .map(|name| DependencySection { kind, name })
    })
}

fn dependency_name_for_section(section: &str, section_name: &str) -> Option<Option<String>> {
    let target_section = format!(".{section_name}");
    let named_section_prefix = format!("{section_name}.");
    let named_target_section = format!(".{section_name}.");

    if section == section_name
        || (section.starts_with("target.") && section.ends_with(target_section.as_str()))
    {
        return Some(None);
    }

    let raw_name = section
        .strip_prefix(named_section_prefix.as_str())
        .or_else(|| {
            if !section.starts_with("target.") {
                return None;
            }
            section
                .split_once(named_target_section.as_str())
                .map(|(_, name)| name)
        })?;
    let name = raw_name.split('.').next().unwrap_or(raw_name);
    let name = unquote(name).trim();
    (!name.is_empty()).then(|| Some(name.to_string()))
}

fn dependency_names_from_line(
    line: &str,
    workspace_aliases: &WorkspaceDependencyAliases,
) -> BTreeSet<String> {
    let Some((raw_key, value)) = line.split_once('=') else {
        return BTreeSet::new();
    };

    let mut names = BTreeSet::new();
    let key = raw_key.trim();
    let key = key.split_once('.').map(|(prefix, _)| prefix).unwrap_or(key);
    let key = unquote(key).trim();
    if !key.is_empty() {
        insert_dependency_name(&mut names, key);
        if dependency_uses_workspace_alias(raw_key, value) {
            names.extend(workspace_aliases.resolve(key));
        }
    }

    if let Some(package) = inline_toml_string_value(value, "package") {
        insert_dependency_name(&mut names, &package);
    }

    names
}

fn dependency_names_from_table_line(
    line: &str,
    section_dependency_name: &str,
    workspace_aliases: &WorkspaceDependencyAliases,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    insert_dependency_name(&mut names, section_dependency_name);

    let Some((key, value)) = line.split_once('=') else {
        return names;
    };
    if key.trim() == "package" {
        let package = unquote(value.trim()).trim();
        if !package.is_empty() {
            insert_dependency_name(&mut names, package);
        }
    } else if key.trim() == "workspace" && value.trim() == "true" {
        names.extend(workspace_aliases.resolve(section_dependency_name));
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

fn forbidden_production_dependency_reason(dep: &str, line: &str) -> Option<&'static str> {
    if is_suspicious_runtime_json_dependency(dep, line) {
        return Some("runtime JSON wire defaults");
    }
    if is_forbidden_sql_dependency(dep, line) {
        return Some("ad hoc SQL surface drift");
    }
    if is_forbidden_native_layout_dependency(dep, line) {
        return Some("implicit native-layout serialization");
    }
    None
}

fn is_forbidden_grpc_or_tonic_dependency_name(value: &str) -> bool {
    let lower = normalize_dependency_name(value);
    lower.contains("tonic")
        || lower.contains("grpc")
        || matches!(
            lower.as_str(),
            "grpcio"
                | "grpcio-sys"
                | "tonic-build"
                | "tonic-prost"
                | "tonic-prost-build"
                | "tonic-web"
                | "tonic-transport"
        )
}

fn is_suspicious_runtime_json_dependency(dep: &str, line: &str) -> bool {
    dependency_or_line_matches_alias(
        dep,
        line,
        &[
            "json",
            "json-rpc",
            "jsonrpc",
            "jsonrpc-core",
            "jsonrpsee",
            "serde-json",
            "serde-json-core",
            "serde_json",
            "simd-json",
            "sonic-rs",
        ],
    )
}

fn is_forbidden_sql_dependency(dep: &str, line: &str) -> bool {
    dependency_or_line_matches_alias(
        dep,
        line,
        &[
            "diesel",
            "mysql",
            "mysql-async",
            "postgres",
            "rusqlite",
            "sea-orm",
            "sea-query",
            "sqlx",
            "tokio-postgres",
        ],
    )
}

fn is_forbidden_native_layout_dependency(dep: &str, line: &str) -> bool {
    dependency_or_line_matches_alias(
        dep,
        line,
        &[
            "abomonation",
            "bincode",
            "bitcode",
            "borsh",
            "bytemuck",
            "postcard",
            "rkyv",
            "speedy",
            "zerocopy",
        ],
    )
}

fn dependency_or_line_matches_alias(dep: &str, line: &str, aliases: &[&str]) -> bool {
    let dep = normalize_dependency_name(dep);
    let line = normalize_dependency_name(line);
    aliases.iter().any(|alias| {
        let alias = normalize_dependency_name(alias);
        dep == alias
            || line.contains(&format!("package=\"{alias}\""))
            || line.contains(&format!("package='{alias}'"))
    })
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

fn insert_dependency_name(names: &mut BTreeSet<String>, name: &str) {
    let name = normalize_dependency_name(name);
    if !name.is_empty() {
        names.insert(name);
    }
}

fn normalize_dependency_name(name: &str) -> String {
    let name = name.trim_matches(|character: char| {
        character == '"' || character == '\'' || character.is_whitespace()
    });
    let name = name.split_once('.').map_or(name, |(key, _)| key);
    name.replace('_', "-").to_ascii_lowercase()
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '#' if !in_single_quote && !in_double_quote => return &line[..index],
            _ => {},
        }
    }
    line
}

#[test]
fn crate_name_topology_bucket_detection_rejects_generic_names() {
    assert_eq!(
        crate_name_uses_generic_topology_bucket("andromeda-common"),
        Some("common")
    );
    assert_eq!(
        crate_name_uses_generic_topology_bucket("andromeda-runtime-utils"),
        Some("utils")
    );
    assert_eq!(
        crate_name_uses_generic_topology_bucket("andromeda-god_engine"),
        Some("god_engine")
    );
    assert_eq!(
        crate_name_uses_generic_topology_bucket("andromeda-wal"),
        None
    );
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
