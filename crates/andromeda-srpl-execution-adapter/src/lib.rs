#![forbid(unsafe_code)]

//! Future SRPL execution adapter owner.
//!
//! This scaffold intentionally contains no behavior. Existing execution adapter
//! contracts remain in `andromeda-srpl` until a later behavior-preserving
//! extraction moves them behind this crate boundary.
//!
//! Dependency direction:
//! - define runtime-free adapter contracts over typed SRPL IR and Procedure
//!   contract shapes;
//! - allow concrete execution owners to depend on this boundary;
//! - avoid parser, binder, lowering, optimizer, storage, transaction, WAL,
//!   transport, benchmark, analytics, GPU, and application-surface
//!   dependencies.
