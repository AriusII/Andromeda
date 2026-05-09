#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Digest

Deterministic digest primitives used by catalog, protocol, and policy
fingerprints.
"#]

mod digest;

#[doc(inline)]
pub use digest::{Sha256, sha256};
