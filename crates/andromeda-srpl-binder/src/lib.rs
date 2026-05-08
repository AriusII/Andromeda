#![forbid(unsafe_code)]

//! Future SRPL binder owner.
//!
//! This scaffold intentionally contains no behavior. Existing binder code
//! remains in `andromeda-srpl` until a later behavior-preserving extraction
//! moves it behind this crate boundary.
//!
//! Dependency direction:
//! - consume parser and AST output from lower SRPL language-model crates;
//! - emit bound semantic data for lowering;
//! - avoid execution, storage, transaction, WAL, transport, benchmark,
//!   analytics, GPU, and application-surface dependencies.
