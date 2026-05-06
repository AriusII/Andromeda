#![forbid(unsafe_code)]

//! Benchmark history store — in-memory, workload-scoped, JSON Lines serializable.
//!
//! The store holds all benchmark run records grouped by workload ID. Records are
//! appended and never mutated. The store can be serialized to JSON Lines and
//! deserialized back without loss of data, enabling portable artifact storage.
//!
//! ## Invariants
//!
//! - Records are append-only; no record is removed or modified after insertion.
//! - `save()` output is a valid JSON Lines document that `import_json_lines()` accepts.
//! - `workload_ids()` returns IDs in sorted order for deterministic output.

use std::collections::HashMap;

use crate::benchmark_history::{BenchmarkHistoryRecord, HistoryQuery, HistoryQueryResult};

/// Benchmark history store keyed by workload ID.
///
/// TECH-DEBT: Context: `from_file` and `save` operate on in-memory state only.
/// A file-backed or artifact-store-backed variant requires async I/O that is out of scope
/// for the current bounded benchmark contracts.
/// Risk: History is lost between process invocations unless the caller serializes via `save()`.
/// Closure: Introduce a persistent backend when CI artifact integration is required.
#[derive(Debug, Clone)]
pub struct BenchmarkHistoryStore {
    records_by_workload: HashMap<String, Vec<BenchmarkHistoryRecord>>,
    storage_path: String,
}

impl BenchmarkHistoryStore {
    pub fn new(storage_path: String) -> Self {
        Self {
            records_by_workload: HashMap::new(),
            storage_path,
        }
    }

    /// Create a store associated with the given path.
    ///
    /// No I/O is performed; the path is recorded for future serialization use.
    pub fn from_file(path: &str) -> Result<Self, String> {
        Ok(Self::new(path.to_string()))
    }

    /// Append a single record to the store.
    pub fn append(&mut self, record: BenchmarkHistoryRecord) -> Result<(), String> {
        self.records_by_workload
            .entry(record.workload_id.clone())
            .or_insert_with(Vec::new)
            .push(record);
        Ok(())
    }

    /// Append multiple records in one call.
    pub fn append_batch(&mut self, records: Vec<BenchmarkHistoryRecord>) -> Result<(), String> {
        for record in records {
            self.append(record)?;
        }
        Ok(())
    }

    /// Query history for a specific workload.
    ///
    /// Returns an error if no records exist for the requested workload ID.
    pub fn query(
        &self,
        workload_id: &str,
        query: HistoryQuery,
    ) -> Result<HistoryQueryResult, String> {
        let all_records = self
            .records_by_workload
            .get(workload_id)
            .ok_or_else(|| format!("no history for workload: {}", workload_id))?;

        let total_in_store = all_records.len();

        let filtered = match query {
            HistoryQuery::LastNCommits(n) => {
                let start = if all_records.len() > n {
                    all_records.len() - n
                } else {
                    0
                };
                all_records[start..].to_vec()
            }

            HistoryQuery::TimeRange(range) => all_records
                .iter()
                .filter(|r| range.contains(&r.timestamp))
                .cloned()
                .collect(),

            HistoryQuery::CommitId(commit_id) => all_records
                .iter()
                .filter(|r| r.commit_id == commit_id)
                .cloned()
                .collect(),

            HistoryQuery::AllForWorkload => all_records.clone(),
        };

        Ok(HistoryQueryResult {
            workload_id: workload_id.to_string(),
            records: filtered,
            total_in_store,
        })
    }

    /// Serialize all records to a JSON Lines string.
    ///
    /// The output is suitable for passing back to `import_json_lines`.
    pub fn save(&self) -> Result<String, String> {
        let mut output = String::new();
        for records in self.records_by_workload.values() {
            for record in records {
                output.push_str(&record.to_json_line());
                output.push('\n');
            }
        }
        Ok(output)
    }

    /// Returns all workload IDs present in the store, sorted.
    pub fn workload_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.records_by_workload.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Returns the number of records for a given workload, or `None` if not present.
    pub fn record_count(&self, workload_id: &str) -> Option<usize> {
        self.records_by_workload
            .get(workload_id)
            .map(|records| records.len())
    }

    /// Returns the total number of records across all workloads.
    pub fn total_records(&self) -> usize {
        self.records_by_workload.values().map(|r| r.len()).sum()
    }

    /// Removes all records from the store.
    pub fn clear(&mut self) {
        self.records_by_workload.clear();
    }

    /// Import records from a JSON Lines string.
    ///
    /// Blank lines are skipped. Returns the count of records successfully imported.
    pub fn import_json_lines(&mut self, content: &str) -> Result<usize, String> {
        let mut count = 0;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record = BenchmarkHistoryRecord::from_json_line(line)?;
            self.append(record)?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TimeRange;

    #[test]
    fn test_store_creation() {
        let store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());
        assert_eq!(store.total_records(), 0);
        assert!(store.workload_ids().is_empty());
    }

    #[test]
    fn test_append_single_record() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        let record = BenchmarkHistoryRecord::new(
            "test-workload".to_string(),
            "commit1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        );

        store.append(record).unwrap();
        assert_eq!(store.total_records(), 1);
        assert_eq!(store.record_count("test-workload"), Some(1));
    }

    #[test]
    fn test_append_multiple_workloads() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        store
            .append(BenchmarkHistoryRecord::new(
                "workload-a".to_string(),
                "c1".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ))
            .unwrap();

        store
            .append(BenchmarkHistoryRecord::new(
                "workload-b".to_string(),
                "c2".to_string(),
                "2026-01-15T13:00:00Z".to_string(),
                200,
                600,
                0,
                10,
            ))
            .unwrap();

        assert_eq!(store.total_records(), 2);
        assert_eq!(store.workload_ids(), vec!["workload-a", "workload-b"]);
    }

    #[test]
    fn test_query_last_n_commits() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        for i in 1..=5 {
            store
                .append(BenchmarkHistoryRecord::new(
                    "test".to_string(),
                    format!("c{}", i),
                    format!("2026-01-15T{:02}:00:00Z", 12 + i),
                    100 + (i as u64) * 10,
                    500,
                    0,
                    10,
                ))
                .unwrap();
        }

        let result = store.query("test", HistoryQuery::LastNCommits(3)).unwrap();

        assert_eq!(result.records.len(), 3);
        assert_eq!(result.records[0].commit_id, "c3");
        assert_eq!(result.records[2].commit_id, "c5");
    }

    #[test]
    fn test_query_time_range() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c1".to_string(),
                "2026-01-15T10:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ))
            .unwrap();

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c2".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                110,
                510,
                0,
                10,
            ))
            .unwrap();

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c3".to_string(),
                "2026-01-15T14:00:00Z".to_string(),
                120,
                520,
                0,
                10,
            ))
            .unwrap();

        let range = TimeRange::new(
            "2026-01-15T11:00:00Z".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
        );

        let result = store.query("test", HistoryQuery::TimeRange(range)).unwrap();

        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].commit_id, "c2");
    }

    #[test]
    fn test_query_specific_commit() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "abc123".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ))
            .unwrap();

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "def456".to_string(),
                "2026-01-15T13:00:00Z".to_string(),
                110,
                510,
                0,
                10,
            ))
            .unwrap();

        let result = store
            .query("test", HistoryQuery::CommitId("def456".to_string()))
            .unwrap();

        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].commit_id, "def456");
        assert_eq!(result.records[0].p50_latency_us, 110);
    }

    #[test]
    fn test_save_json_lines() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c1".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ))
            .unwrap();

        let json = store.save().unwrap();
        assert!(json.contains("\"workload_id\":\"test\""));
        assert!(json.contains("\"commit_id\":\"c1\""));
    }

    #[test]
    fn test_import_json_lines() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        let json_lines = r#"{"workload_id":"test","commit_id":"c1","timestamp":"2026-01-15T12:00:00Z","p50_latency_us":100,"p95_latency_us":500,"error_count":0,"sample_count":10,"branch":null,"pr_number":null}
{"workload_id":"test","commit_id":"c2","timestamp":"2026-01-15T13:00:00Z","p50_latency_us":110,"p95_latency_us":510,"error_count":0,"sample_count":10,"branch":null,"pr_number":null}"#;

        let count = store.import_json_lines(json_lines).unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.total_records(), 2);

        let result = store.query("test", HistoryQuery::AllForWorkload).unwrap();
        assert_eq!(result.records.len(), 2);
    }

    #[test]
    fn test_query_unknown_workload() {
        let store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());
        let result = store.query("nonexistent", HistoryQuery::AllForWorkload);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_append() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        let records = vec![
            BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c1".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ),
            BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c2".to_string(),
                "2026-01-15T13:00:00Z".to_string(),
                110,
                510,
                0,
                10,
            ),
        ];

        store.append_batch(records).unwrap();
        assert_eq!(store.total_records(), 2);
    }

    #[test]
    fn test_clear() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        store
            .append(BenchmarkHistoryRecord::new(
                "test".to_string(),
                "c1".to_string(),
                "2026-01-15T12:00:00Z".to_string(),
                100,
                500,
                0,
                10,
            ))
            .unwrap();

        assert_eq!(store.total_records(), 1);
        store.clear();
        assert_eq!(store.total_records(), 0);
    }
}
