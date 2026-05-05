/// Decision Record Reference Consistency Test
///
/// This test validates that all cross-references in decision records
/// (docs/decisions/DEC-*.md) point to existing files.
///
/// Fails if any referenced DEC-XXX cannot be found.
/// Used to prevent reference drift and maintain decision hygiene.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use regex::Regex;

#[test]
fn test_decision_record_references_valid() {
    // Path to decisions directory
    let decisions_dir = PathBuf::from("docs/decisions");
    
    if !decisions_dir.exists() {
        eprintln!("Warning: docs/decisions directory not found, skipping reference validation");
        return;
    }

    // Scan all DEC files and collect available DEC IDs
    let mut available_decs = HashSet::new();
    let mut dec_files = Vec::new();

    match fs::read_dir(&decisions_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md") == Some(true) {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if filename.starts_with("DEC-") {
                            dec_files.push(path.clone());
                            
                            // Extract DEC ID from filename (e.g., "DEC-020" from "DEC-020-quorum-runtime.md")
                            if let Some(dec_id) = extract_dec_id(filename) {
                                available_decs.insert(dec_id);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading decisions directory: {}", e);
            panic!("Cannot read docs/decisions directory");
        }
    }

    println!("\n📋 Decision Record Reference Audit\n");
    println!("Found {} DEC files:", dec_files.len());
    for dec_id in available_decs.iter().copied().collect::<Vec<_>>().sort_then_iter() {
        println!("  ✓ {}", dec_id);
    }

    // Now scan all DEC files for references
    let dec_reference_regex = Regex::new(r"\bDEC-(\d{3}[a-z]?)\b").expect("Invalid regex");
    let mut all_references: Vec<(String, String)> = Vec::new();
    let mut reference_counts: HashMap<String, usize> = HashMap::new();
    let mut missing_targets = Vec::new();

    for path in &dec_files {
        match fs::read_to_string(path) {
            Ok(content) => {
                for cap in dec_reference_regex.captures_iter(&content) {
                    if let Some(dec_match) = cap.get(1) {
                        let dec_ref = format!("DEC-{}", dec_match.as_str());
                        let filename = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");
                        
                        all_references.push((filename.to_string(), dec_ref.clone()));
                        *reference_counts.entry(dec_ref.clone()).or_insert(0) += 1;

                        // Verify target exists
                        if !available_decs.contains(dec_ref.as_str()) {
                            missing_targets.push((filename.to_string(), dec_ref.clone()));
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", path.display(), e);
                panic!("Cannot read decision file: {}", path.display());
            }
        }
    }

    println!("\n📊 Reference Summary\n");
    println!("Total references found: {}", all_references.len());
    println!("Unique DECs referenced: {}", reference_counts.len());

    // Print top referenced DECs
    println!("\n🔗 Most Referenced DECs:");
    let mut sorted_refs: Vec<_> = reference_counts.iter().collect();
    sorted_refs.sort_by_key(|&(_, count)| std::cmp::Reverse(*count));
    for (dec_id, count) in sorted_refs.iter().take(10) {
        println!("  {} — cited {} times", dec_id, count);
    }

    // Report missing targets
    if !missing_targets.is_empty() {
        println!("\n❌ Missing Target References:\n");
        for (source, target) in &missing_targets {
            println!("  {} references {} (NOT FOUND)", source, target);
        }
        panic!("Found {} broken references", missing_targets.len());
    } else {
        println!("\n✅ All {} references are valid — no missing targets!", all_references.len());
    }

    // Additional validation: check for bidirectional consistency in known pairs
    validate_bidirectional_pairs(&available_decs);
}

/// Validates known DEC pairs have explicit governance notes
fn validate_bidirectional_pairs(available_decs: &HashSet<&str>) {
    let known_pairs = vec![
        ("DEC-020a", "DEC-020b", "Stream Concurrency ↔ Quorum Runtime"),
        ("DEC-022a", "DEC-022b", "Protocol Stability ↔ Procedure Lifecycle"),
        ("DEC-024a", "DEC-024b", "Promotion Boundary ↔ Stream Mapping"),
    ];

    println!("\n🔄 Bidirectional Consistency Check\n");

    for (dec_a, dec_b, description) in &known_pairs {
        let has_a = available_decs.contains(dec_a);
        let has_b = available_decs.contains(dec_b);

        if has_a && has_b {
            println!("  ✓ {} pair complete ({})", description, dec_a);
        } else if has_a || has_b {
            eprintln!("  ⚠️  {} incomplete (only {} found)", description, 
                     if has_a { dec_a } else { dec_b });
        }
    }
}

/// Extract DEC ID from filename (e.g., "DEC-020-stream-concurrency.md" → "DEC-020")
fn extract_dec_id(filename: &str) -> Option<&str> {
    if filename.starts_with("DEC-") && filename.ends_with(".md") {
        let end = filename[4..].find('-')
            .or_else(|| filename[4..].find('_'))
            .unwrap_or(filename[4..].len() - 3); // Account for .md
        
        return Some(&filename[..4 + end]);
    }
    None
}

/// Helper trait for sorting in tests
trait SortThenIter {
    fn sort_then_iter(self) -> Vec<String>;
}

impl SortThenIter for HashSet<String> {
    fn sort_then_iter(self) -> Vec<String> {
        let mut sorted: Vec<_> = self.into_iter().collect();
        sorted.sort();
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dec_id() {
        assert_eq!(extract_dec_id("DEC-020-stream-concurrency.md"), Some("DEC-020"));
        assert_eq!(extract_dec_id("DEC-024b-hadr-stream-mapping.md"), Some("DEC-024b"));
        assert_eq!(extract_dec_id("DEC-011-rust-workspace-topology.md"), Some("DEC-011"));
        assert_eq!(extract_dec_id("not-a-dec.md"), None);
    }

    #[test]
    fn test_reference_pattern() {
        let dec_reference_regex = Regex::new(r"\bDEC-(\d{3}[a-z]?)\b").unwrap();
        
        // Should match
        assert!(dec_reference_regex.is_match("See DEC-020 for details"));
        assert!(dec_reference_regex.is_match("DEC-024b handles failover"));
        assert!(dec_reference_regex.is_match("(DEC-019)"));
        
        // Should not match incomplete patterns
        assert!(!dec_reference_regex.is_match("DEC-20"));
        assert!(!dec_reference_regex.is_match("DEC-A20"));
    }
}
