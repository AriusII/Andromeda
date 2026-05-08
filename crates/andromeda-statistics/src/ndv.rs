use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub const DEFAULT_HLL_PRECISION: u8 = 12;
pub const NDV_EXACT_THRESHOLD: u64 = 10_000;

pub trait NdvEstimator: Send + Sync {
    fn observe(&mut self, value: u64);
    fn estimate(&self) -> u64;
    fn memory_bytes(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct ExactNdvCounter {
    seen: std::collections::HashSet<u64>,
}

impl ExactNdvCounter {
    pub fn new() -> Self {
        Self {
            seen: std::collections::HashSet::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: std::collections::HashSet::with_capacity(capacity),
        }
    }
}

impl Default for ExactNdvCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl NdvEstimator for ExactNdvCounter {
    fn observe(&mut self, value: u64) {
        self.seen.insert(value);
    }

    fn estimate(&self) -> u64 {
        self.seen.len() as u64
    }

    fn memory_bytes(&self) -> usize {
        self.seen.capacity() * std::mem::size_of::<u64>()
    }
}

#[derive(Debug, Clone)]
pub struct HyperLogLog {
    precision: u8,
    registers: Vec<u8>,
    alpha: f64,
}

impl HyperLogLog {
    pub fn new(precision: u8) -> AndromedaResult<Self> {
        if !(4..=16).contains(&precision) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "HyperLogLog precision must be in range [4, 16]",
            ));
        }

        let m = (1u64 << precision) as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let registers = vec![0u8; 1 << precision];

        Ok(Self {
            precision,
            registers,
            alpha,
        })
    }

    pub fn with_default_precision() -> AndromedaResult<Self> {
        Self::new(DEFAULT_HLL_PRECISION)
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub fn register_count(&self) -> usize {
        1 << self.precision
    }

    pub fn error_percent(&self) -> f64 {
        (1.04 / (1u64 << self.precision) as f64).sqrt() * 100.0
    }
}

impl NdvEstimator for HyperLogLog {
    fn observe(&mut self, value: u64) {
        let hash = splitmix64(value);
        let index = (hash >> (64 - self.precision)) as usize;
        let rank = hll_rank(hash, self.precision);
        self.registers[index] = self.registers[index].max(rank);
    }

    fn estimate(&self) -> u64 {
        let register_count = self.register_count() as f64;
        let harmonic_sum: f64 = self
            .registers
            .iter()
            .map(|rank| 2.0_f64.powi(-(*rank as i32)))
            .sum();
        let raw_estimate = self.alpha * register_count * register_count / harmonic_sum;
        let zero_registers = self.registers.iter().filter(|rank| **rank == 0).count();

        let estimate = if raw_estimate <= 2.5 * register_count && zero_registers > 0 {
            register_count * (register_count / zero_registers as f64).ln()
        } else {
            raw_estimate
        };

        estimate.round() as u64
    }

    fn memory_bytes(&self) -> usize {
        self.registers.len()
    }
}

fn hll_rank(hash: u64, precision: u8) -> u8 {
    let remaining = hash << precision;
    let max_rank = 64 - precision as u32 + 1;
    (remaining.leading_zeros() + 1).min(max_rank) as u8
}

fn splitmix64(value: u64) -> u64 {
    let mut x = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}
