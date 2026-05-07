use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::support::sorted_child_paths;

#[derive(Debug, Default)]
pub(crate) struct DependencyManifest {
    pub(crate) production_deps: BTreeSet<String>,
    pub(crate) dev_deps: BTreeSet<String>,
    pub(crate) forbidden_wire_deps: Vec<String>,
}

pub(crate) fn collect_dependency_manifests(
    workspace: &Path,
) -> BTreeMap<String, DependencyManifest> {
    let mut manifests = BTreeMap::new();
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
            parse_dependency_manifest(&crate_name, &text),
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
            dependency_names_from_table_line(trimmed, section_dependency_name)
        } else {
            dependency_names_from_line(trimmed)
        };
        if dependency_names.is_empty() {
            continue;
        }

        if dependency_section.kind == DependencySectionKind::Production {
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
        } else if dependency_section.kind == DependencySectionKind::Dev {
            manifest.dev_deps.extend(dependency_names.iter().cloned());
        }

        if is_forbidden_grpc_or_tonic_dependency(trimmed)
            || dependency_names
                .iter()
                .any(|dep| is_forbidden_grpc_or_tonic_dependency(dep))
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

fn dependency_names_from_table_line(line: &str, section_dependency_name: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::from([section_dependency_name.to_string()]);
    let Some((key, value)) = line.split_once('=') else {
        return names;
    };
    if key.trim() == "package" {
        let package = unquote(value.trim()).trim();
        if !package.is_empty() {
            names.insert(package.to_string());
        }
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
