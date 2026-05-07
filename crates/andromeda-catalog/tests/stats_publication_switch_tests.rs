//! Statistics publication switch contract tests.
//!
//! These tests keep active `StatsVersion` transitions tied to explicit
//! canonical or recovery evidence. Predictive scenario evidence can remain
//! attached as advisory identity, but it must not drive publication or rollback
//! by itself.

#[path = "stats_publication_switch_tests/advisory_identity.rs"]
mod advisory_identity;
#[path = "stats_publication_switch_tests/mod.rs"]
mod common;
#[path = "stats_publication_switch_tests/decision_evidence.rs"]
mod decision_evidence;
#[path = "stats_publication_switch_tests/lifecycle.rs"]
mod lifecycle;
#[path = "stats_publication_switch_tests/trace_and_bounds.rs"]
mod trace_and_bounds;
