#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Time

Clock abstraction and deterministic engine timestamp primitives.
"#]

mod time;

pub use time::{Clock, EngineTimestamp, ManualClock, SystemClock};
