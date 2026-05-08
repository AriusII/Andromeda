//! Compatibility reexport for the WAL commit log facade.
//!
//! The canonical durability-before-visibility gate lives in
//! `andromeda_wal::write_ahead_log::commit_log_facade`.

pub use andromeda_wal::CommitLogFacade;
