#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Time

Clock abstraction and deterministic engine timestamp primitives.
"#]

mod time;

#[doc(inline)]
pub use time::{Clock, EngineTimestamp, ManualClock, SystemClock};
