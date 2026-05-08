#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Codec

Explicit little-endian byte helpers for runtime-free binary contracts.
"#]

mod error;
mod little_endian;

pub use error::{CodecBoundsError, CodecError, CodecResult};
pub use little_endian::{
    read_exact, read_u8, read_u16_le, read_u32_le, read_u64_le, read_u128_le, write_exact,
    write_u8, write_u16_le, write_u32_le, write_u64_le, write_u128_le,
};
