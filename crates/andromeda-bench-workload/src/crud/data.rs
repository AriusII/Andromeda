/// Deterministic data generator for reproducible benchmarks.
#[derive(Debug, Clone)]
pub struct CrudDataGenerator {
    seed: u64,
    row_size_bytes: usize,
}

impl CrudDataGenerator {
    pub fn new(seed: u64, row_size_bytes: usize) -> Self {
        Self {
            seed,
            row_size_bytes: row_size_bytes.max(64),
        }
    }

    /// Generate deterministic rows using an LCG-style PRNG.
    pub fn generate_rows(&self, count: u32) -> Vec<CrudRow> {
        let mut rows = Vec::with_capacity(count as usize);
        let mut state = self.seed;

        for i in 0..count {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let payload = vec![((state >> (i % 64)) & 0xFF) as u8; self.row_size_bytes];
            rows.push(CrudRow {
                id: state,
                payload,
                timestamp: u64::from(i),
            });
        }

        rows
    }
}

/// A row in the CRUD benchmark workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudRow {
    pub id: u64,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}
