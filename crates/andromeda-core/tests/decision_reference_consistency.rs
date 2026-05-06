use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn decision_record_references_point_to_existing_decisions() {
    let decisions_dir = PathBuf::from("docs/decisions");
    if !decisions_dir.exists() {
        return;
    }

    let (available_decs, dec_files) = collect_decision_files(&decisions_dir);
    let missing_targets = collect_missing_references(&available_decs, &dec_files);

    assert!(
        missing_targets.is_empty(),
        "broken decision references: {missing_targets:?}"
    );
    assert_bidirectional_pairs(&available_decs);
}

fn collect_decision_files(decisions_dir: &Path) -> (HashSet<String>, Vec<PathBuf>) {
    let mut available_decs = HashSet::new();
    let mut dec_files = Vec::new();

    let entries = fs::read_dir(decisions_dir).expect("docs/decisions must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md")
            && let Some(filename) = path.file_name().and_then(|name| name.to_str())
            && filename.starts_with("DEC-")
        {
            dec_files.push(path.clone());
            if let Some(dec_id) = extract_dec_id(filename) {
                available_decs.insert(dec_id.to_string());
            }
        }
    }

    (available_decs, dec_files)
}

fn collect_missing_references(
    available_decs: &HashSet<String>,
    dec_files: &[PathBuf],
) -> Vec<(String, String)> {
    let mut missing_targets = Vec::new();

    for path in dec_files {
        let content = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        for dec_ref in extract_dec_references(&content) {
            if !available_decs.contains(dec_ref.as_str()) {
                let filename = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                missing_targets.push((filename, dec_ref));
            }
        }
    }

    missing_targets
}

fn assert_bidirectional_pairs(available_decs: &HashSet<String>) {
    let known_pairs = [
        (
            "DEC-020",
            "DEC-020b",
            "quorum runtime and stream concurrency",
        ),
        (
            "DEC-022",
            "DEC-022b",
            "procedure lifecycle and protocol stability",
        ),
        (
            "DEC-024",
            "DEC-024b",
            "promotion boundary and stream mapping",
        ),
    ];

    for (dec_a, dec_b, description) in known_pairs {
        let has_a = available_decs.contains(dec_a);
        let has_b = available_decs.contains(dec_b);
        assert_eq!(
            has_a, has_b,
            "decision pair for {description} must be complete: {dec_a}={has_a}, {dec_b}={has_b}"
        );
    }
}

fn extract_dec_id(filename: &str) -> Option<&str> {
    if filename.starts_with("DEC-") && filename.ends_with(".md") {
        let end = filename[4..]
            .find('-')
            .or_else(|| filename[4..].find('_'))
            .unwrap_or(filename[4..].len() - 3);

        return Some(&filename[..4 + end]);
    }
    None
}

fn extract_dec_references(content: &str) -> Vec<String> {
    content
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .filter_map(|token| {
            let suffix = token.strip_prefix("DEC-")?;
            let bytes = suffix.as_bytes();
            if bytes.len() < 3 || !bytes[..3].iter().all(u8::is_ascii_digit) {
                return None;
            }
            let suffix_len = if bytes.get(3).is_some_and(u8::is_ascii_lowercase) {
                4
            } else {
                3
            };
            Some(format!("DEC-{}", &suffix[..suffix_len]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dec_id() {
        assert_eq!(
            extract_dec_id("DEC-020-stream-concurrency.md"),
            Some("DEC-020")
        );
        assert_eq!(
            extract_dec_id("DEC-024b-hadr-stream-mapping.md"),
            Some("DEC-024b")
        );
        assert_eq!(
            extract_dec_id("DEC-011-rust-workspace-topology.md"),
            Some("DEC-011")
        );
        assert_eq!(extract_dec_id("not-a-dec.md"), None);
    }

    #[test]
    fn test_reference_pattern() {
        assert_eq!(
            extract_dec_references("See DEC-020 for details"),
            vec!["DEC-020"]
        );
        assert_eq!(
            extract_dec_references("DEC-024b handles failover"),
            vec!["DEC-024b"]
        );
        assert_eq!(extract_dec_references("(DEC-019)"), vec!["DEC-019"]);

        assert!(extract_dec_references("DEC-20").is_empty());
        assert!(extract_dec_references("DEC-A20").is_empty());
    }
}
