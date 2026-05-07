#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Error

Typed error categories and result alias shared by foundation crates.
"#]

mod error;

pub use error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
