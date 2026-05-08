#![forbid(unsafe_code)]

//! Future SRPL lowering owner.
//!
//! This scaffold intentionally contains no behavior. Existing lowering code
//! remains in `andromeda-srpl` until a later behavior-preserving extraction
//! moves it behind this crate boundary.
//!
//! Dependency direction:
//! - consume bound SRPL semantic input and lower it into typed IR;
//! - depend only on contract-safe language-model crates and foundation crates;
//! - avoid parser ownership, catalog storage, execution, storage, transaction,
//!   WAL, transport, benchmark, analytics, GPU, and application-surface
//!   dependencies.
