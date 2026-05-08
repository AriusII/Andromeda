#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Error

Typed error categories and result alias shared by foundation crates.
"#]

mod error;

#[doc(inline)]
pub use error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
