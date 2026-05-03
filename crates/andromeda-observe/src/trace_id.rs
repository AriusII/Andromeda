#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TraceId(u128);

impl TraceId {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_ids_are_stable_values() {
        let trace_id = TraceId::new(99);
        assert_eq!(trace_id.get(), 99);
    }
}
