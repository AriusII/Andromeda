//! Two-phase locking (2PL) protocol definition for Andromeda.
//!
//! # Overview
//!
//! Andromeda enforces **strict two-phase locking (2PL)** to guarantee serializability
//! under MVCC isolation. All locks for a transaction must be acquired during a
//! single growing phase before any lock is released in a shrinking phase.
//!
//! This module documents the protocol, lock compatibility rules, and error contracts.
//! The [`crate::lock_manager::LockManager`] implements this protocol.
//!
//! # Two-Phase Locking Phases
//!
//! ## Growing Phase
//!
//! Transactions acquire locks on resources as needed for reads and writes:
//! - Row reads: acquire Shared locks on row resources
//! - Row writes: acquire Exclusive locks on row resources
//! - Catalog DefinitionBatch changes: acquire SchemaExclusive locks on table resources
//! - Table scans: acquire IntentShared on table, then Shared on rows
//!
//! Locks can be upgraded (Shared → Exclusive, IntentShared → IntentExclusive)
//! if no incompatible waiter blocks the upgrade.
//!
//! ## Shrinking Phase
//!
//! After the transaction receives a durable commit or rollback decision,
//! it enters the shrinking phase and releases all locks via [`crate::lock_manager::LockManager::release_all`].
//!
//! **Critical:** No early lock release is permitted. Early release breaks the 2PL
//! guarantee and can cause:
//! - Phantom reads (concurrent INSERT violates repeatable read)
//! - Non-repeatable reads (concurrent UPDATE violates read stability)
//! - Dirty reads (if isolation level is compromised)
//!
//! # Lock Modes and Compatibility Matrix
//!
//! ## Lock Mode Definitions
//!
//! | Mode | Name | Purpose | Coexistence |
//! |:---:|:---|:---|:---|
//! | **S** | Shared | Concurrent read-only access | Compatible with S, IS, SS |
//! | **X** | Exclusive | Exclusive write access | Exclusive access only |
//! | **IS** | Intent Shared | Intent to read sub-resources | Compatible with IS, IX, S, SS |
//! | **IX** | Intent Exclusive | Intent to write sub-resources | Compatible with IS, IX, SS |
//! | **SS** | Schema Shared | Schema stability (DefinitionBatch blocked) | Compatible with S, IS, IX, SS |
//! | **SX** | Schema Exclusive | Exclusive catalog access (DefinitionBatch) | Exclusive access only |
//!
//! ## Full Compatibility Matrix
//!
//! Rows: existing holder lock mode. Columns: requested lock mode.
//!
//! | Existing \ Requested | S  | X  | IS | IX | SS | SX |
//! |:---:|:---:|:---:|:---:|:---:|:---:|:---:|
//! | **S**  | ✅ | ❌ | ✅ | ❌ | ✅ | ❌ |
//! | **X**  | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
//! | **IS** | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ |
//! | **IX** | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ |
//! | **SS** | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ |
//! | **SX** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
//!
//! **Key Observations:**
//! - Exclusive (X) conflicts with all other modes.
//! - Schema Exclusive (SX) conflicts with all other modes.
//! - Shared (S) allows concurrent readers and intent readers.
//! - Intent modes (IS, IX) coordinate multi-granularity locking.
//!
//! # Hierarchical Locking (Multi-Granularity)
//!
//! Andromeda locks resources at multiple granularity levels:
//! ```text
//! Schema
//!   │
//!   ├─→ Table (requires intent lock on schema)
//!   │     │
//!   │     ├─→ Page (requires intent lock on table and schema)
//!   │     │     │
//!   │     │     └─→ Row (requires intent lock on page, table, and schema)
//! ```
//!
//! ### Hierarchical Locking Rules
//!
//! 1. **Ancestor Intent Requirement:** To acquire a lock at a finer granularity,
//!    the transaction must hold an intent lock (IS or IX) on all coarser ancestors.
//!    - To lock a row: must hold IX on table and schema
//!    - To lock a page: must hold IX on table and schema
//!    - To lock a table: must hold IX on schema
//!
//! 2. **Intent Lock Promotion:** Intent locks coordinate multi-granularity:
//!    - IS (Intent Shared) signals intent to read sub-resources
//!    - IX (Intent Exclusive) signals intent to modify sub-resources
//!    - Allows finer-granularity locks to be acquired without over-locking ancestors
//!
//! 3. **Lock Compatibility Under Hierarchy:**
//!    - IS is compatible with IS, IX, S, and SS (allows concurrent readers and other writers)
//!    - IX is compatible with IS, IX, and SS (allows other intents and schema stability)
//!    - S conflicts with IX and X (no writers if readers present)
//!    - X and SX are exclusive (no sharing)
//!
//! # Lock Acquisition Protocol (V0)
//!
//! The `LockManager::acquire` method implements the following protocol:
//!
//! ## 1. Immediate Grant
//! If all current holders are compatible with the requested mode, grant immediately.
//!
//! ## 2. Same-Transaction Re-entry
//! If the same transaction already holds the requested lock mode on the resource,
//! return `AlreadyHeld` without creating a duplicate holder.
//!
//! ## 3. Same-Transaction Upgrade
//! If the same transaction holds a weaker mode (e.g., Shared), and the requested
//! mode (e.g., Exclusive) is a valid V0 upgrade, upgrade in place if:
//! - All current holders are compatible with the new mode, AND
//! - No older incompatible waiter blocks the upgrade (FIFO fairness)
//!
//! Valid V0 upgrades:
//! - Shared → Exclusive
//! - IntentShared → IntentExclusive
//! - SchemaShared → SchemaExclusive
//!
//! ## 4. FIFO Fairness
//! If a current incompatible holder exists OR an older incompatible waiter blocks,
//! the request is enqueued as a waiter.
//!
//! Example: Reader holds Shared, new request is Exclusive, earlier waiter requested IX.
//! - The Exclusive waiter blocks because the IX waiter came first (FIFO).
//! - This prevents writer starvation.
//!
//! ## 5. Waiter Queue Semantics
//! Waiters are stored in a FIFO queue (`VecDeque`). When a lock is released:
//! - Compatible waiters at the front are promoted to holders in order
//! - Promotion stops at the first incompatible waiter
//! - No skipping in the queue
//!
//! # Lock Release Protocol
//!
//! ## Single Resource Release
//! [`crate::lock_manager::LockManager::release`] removes all holder and waiter
//! records for a transaction on a single resource, then promotes compatible waiters.
//!
//! ## Terminal Release (release_all)
//! [`crate::lock_manager::LockManager::release_all`] is called **only** at a
//! transaction-end boundary after durable evidence of commit or rollback.
//! It removes all locks across all resources and promotes compatible waiters globally.
//!
//! **Usage Contract:**
//! ```ignore
//! // After durable commit
//! lock_manager.release_all(tx_id)?;
//!
//! // After durable rollback
//! lock_manager.release_all(tx_id)?;
//! ```
//!
//! **Violation:** Calling `release_all` before commit/rollback decision is durable
//! breaks 2PL and risks phantom reads.
//!
//! # Error Handling
//!
//! All lock manager operations return [`andromeda_core::AndromedaResult`].
//! No panics are permitted.
//!
//! ## Error Cases
//!
//! | Condition | Error Kind | Example |
//! |:---|:---|:---|
//! | Invalid transaction ID (zero) | `ErrorKind::Transaction` | `tx_id = 0` |
//! | Invalid resource ID component (zero) | `ErrorKind::Transaction` | `resource.schema_id = 0` |
//! | Sequence counter overflow | `ErrorKind::Transaction` | 2^64 waiters allocated |
//! | Mutex poison | `ErrorKind::Transaction` | Internal panic in lock manager |
//!
//! ## Error Recovery
//!
//! - **Invalid Input:** Caller should validate inputs before calling lock manager.
//! - **Sequence Overflow:** Extremely rare; indicates transaction aged >2^64 waiter sequences.
//! - **Mutex Poison:** Indicates internal bug; entire transaction may need rollback.
//!
//! # Deferred: Deadlock Detection
//!
//! Deadlock detection (cycle detection in wait-for graph) is deferred to Wave 21 Batch 2 Task 4.
//!
//! The lock manager provides inspection APIs for external deadlock detection:
//! - [`crate::lock_manager::LockManager::snapshot`] — full lock table snapshot
//! - [`crate::lock_manager::LockManager::entry`] — single resource entry
//! - Embedded `holders` and `waiters` in lock entries enable wait-for graph construction
//!
//! # Trace and Audit Evidence
//!
//! The lock manager emits non-breaking trace evidence for:
//! - **Critical waits:** [`crate::lock_manager::LockDecisionEvidence`] with blockers
//! - **Waiter promotions:** [`crate::lock_manager::LockDecisionEvidence::promotion`]
//! - **Terminal cleanup:** [`crate::lock_manager::LockReleaseAllSummary`]
//!
//! Evidence is transaction-local and does not imply durability.
//!
//! # Consistency Guarantees
//!
//! When 2PL is enforced correctly:
//!
//! | Isolation Level | Anomaly Prevention | Mechanism |
//! |:---|:---|:---|
//! | Read Uncommitted | None | No locks acquired |
//! | Read Committed | Dirty read | Shared lock held until commit |
//! | Repeatable Read | Repeatable read | Shared lock held until transaction end |
//! | Serializable | All anomalies | Exclusive locks + range locks (not V0) |
//!
//! In Andromeda:
//! - **Repeatable Read (default):** 2PL with Shared/Exclusive locks ensures read stability.
//! - **Serializable (future):** Range locks (next-key locks) needed for phantom prevention.
//!
//! # References
//!
//! - Eswaran, K. P., et al. "The notions of consistency and predicate locks in a database system."
//!   Communications of the ACM, 1976.
//! - Gray, J., & Reuter, A. "Transaction Processing: Concepts and Techniques." 1993.
