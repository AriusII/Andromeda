//! Compatibility shim for the old machine-output helper module.
//!
//! New code should use `crate::diagnostic_json` directly.

#[allow(unused_imports)]
pub use crate::diagnostic_json::{
    DIAGNOSTIC_JSON_FLAG, JSON_FLAG, json_option_string, json_option_u64, json_string,
    json_string_array, json_u64_array, parse_diagnostic_json_flag, parse_json_flag,
};
