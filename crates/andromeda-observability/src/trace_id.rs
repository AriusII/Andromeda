#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceId(u128);

impl TraceId {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
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

    #[test]
    fn zero_trace_id_is_detectable_for_envelope_validation() {
        assert!(TraceId::new(0).is_zero());
        assert!(!TraceId::new(1).is_zero());
    }
}
