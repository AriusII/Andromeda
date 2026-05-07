//! Compatibility facade for runtime-free protocol invariant validation.

pub use andromeda_rpc_protocol::{
    FrameTypeInvariants, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    validate_frame_header_layout,
};
