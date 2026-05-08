use crate::manifest_loading::{
    cargo_line_without_comment, normalize_dependency_name, parse_package_rename,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
pub(crate) fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}
fn collect_rust_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|err| {
            panic!("failed to read directory entry in {}: {err}", dir.display())
        });
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
pub(crate) fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
pub(crate) fn strip_rust_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0_usize;
    let mut in_line_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            continue;
        }

        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                    output.push_str("  ");
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                    output.push_str("  ");
                }
                ('\n', _) => output.push('\n'),
                _ => output.push(' '),
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('/', Some('/')) => {
                chars.next();
                in_line_comment = true;
                output.push_str("  ");
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
                output.push_str("  ");
            }
            _ => output.push(ch),
        }
    }

    output
}
pub(crate) fn exec_runtime_quinn_feature_violations(manifest_text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let expected = BTreeSet::from(["andromeda-quic/runtime-quinn".to_owned()]);

    match manifest_feature_values(manifest_text, "runtime-quinn") {
        Some(values) if values == expected => {}
        Some(values) => violations.push(format!(
            "andromeda-exec runtime-quinn feature must reexport only andromeda-quic/runtime-quinn; found [{}]",
            values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        None => violations.push(
            "andromeda-exec must declare runtime-quinn = [\"andromeda-quic/runtime-quinn\"]"
                .to_owned(),
        ),
    }

    if manifest_feature_values(manifest_text, "default")
        .is_some_and(|values| values.contains("runtime-quinn"))
    {
        violations.push("andromeda-exec default features must not enable runtime-quinn".to_owned());
    }

    if dependency_line_enables_runtime_quinn_on_andromeda_quic(manifest_text) {
        violations.push(
            "andromeda-exec must not enable andromeda-quic/runtime-quinn from a dependency entry; use the runtime-quinn feature reexport"
                .to_owned(),
        );
    }

    violations
}
fn manifest_feature_values(manifest_text: &str, feature_name: &str) -> Option<BTreeSet<String>> {
    let mut in_features = false;
    let mut collecting = false;
    let mut feature_text = String::new();

    for line in manifest_text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            in_features = line == "[features]";
            collecting = false;
            feature_text.clear();
            continue;
        }

        if !in_features {
            continue;
        }

        if collecting {
            feature_text.push_str(line);
            if line.contains(']') {
                return Some(parse_feature_values(&feature_text));
            }
            continue;
        }

        let Some((raw_name, rest)) = line.split_once('=') else {
            continue;
        };

        if normalize_dependency_name(raw_name.trim()) == feature_name {
            feature_text.push_str(rest);
            if rest.contains(']') {
                return Some(parse_feature_values(&feature_text));
            }
            collecting = true;
        }
    }

    collecting.then(|| parse_feature_values(&feature_text))
}
fn parse_feature_values(feature_text: &str) -> BTreeSet<String> {
    feature_text
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}
fn dependency_line_enables_runtime_quinn_on_andromeda_quic(manifest_text: &str) -> bool {
    let mut in_dependency_section = false;
    let mut named_dependency = None;

    for line in manifest_text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            let section = line.trim_matches(&['[', ']'][..]);
            let Some(dependency_name) = dependency_table_dependency_name(section) else {
                in_dependency_section = false;
                named_dependency = None;
                continue;
            };
            in_dependency_section = true;
            named_dependency = dependency_name;
            continue;
        }

        if !in_dependency_section || !line.contains("runtime-quinn") {
            continue;
        }

        if named_dependency.as_deref() == Some("andromeda-quic") {
            return true;
        }

        let Some((raw_name, rest)) = line.split_once('=') else {
            continue;
        };
        let dependency = normalize_dependency_name(raw_name.trim());
        let package = parse_package_rename(rest).map(|name| normalize_dependency_name(&name));
        if dependency == "andromeda-quic" || package.as_deref() == Some("andromeda-quic") {
            return true;
        }
    }

    false
}
fn dependency_table_dependency_name(section: &str) -> Option<Option<String>> {
    for dependency_section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if section == dependency_section
            || (section.starts_with("target.")
                && section.ends_with(format!(".{dependency_section}").as_str()))
        {
            return Some(None);
        }

        let named_prefix = format!("{dependency_section}.");
        if let Some(raw_name) = section.strip_prefix(named_prefix.as_str()) {
            let name = raw_name.split('.').next().unwrap_or(raw_name);
            return Some(Some(normalize_dependency_name(name)));
        }

        let named_target_section = format!(".{dependency_section}.");
        if section.starts_with("target.")
            && let Some((_, raw_name)) = section.split_once(named_target_section.as_str())
        {
            let name = raw_name.split('.').next().unwrap_or(raw_name);
            return Some(Some(normalize_dependency_name(name)));
        }
    }

    None
}
