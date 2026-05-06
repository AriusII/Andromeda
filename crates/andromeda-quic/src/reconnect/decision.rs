#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectDecision {
    RetryAfter { next_attempt: u32, delay_ms: u64 },
    GiveUp,
}
