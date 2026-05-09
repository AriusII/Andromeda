crate::define_observability_u128_id!(TraceId);

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
