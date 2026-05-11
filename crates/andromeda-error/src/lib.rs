#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Error

Typed error categories and result alias shared by foundation crates.
"#]

mod classification;
mod error;

#[doc(inline)]
pub use classification::{ExecutionErrorClass, classify_andromeda_error};
#[doc(inline)]
pub use error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
