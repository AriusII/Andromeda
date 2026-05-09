/// Lock mode vocabulary for transaction concurrency control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockMode {
    Shared,
    Exclusive,
    IntentShared,
    IntentExclusive,
    SchemaShared,
    SchemaExclusive,
}

impl LockMode {
    /// Return whether an already-held lock mode can coexist with a newly
    /// requested lock mode on the same resource under the V0 matrix.
    ///
    /// The check is deterministic and transaction-agnostic: it deliberately
    /// treats `existing` and `requested` as locks held/requested by different
    /// transactions. Same-transaction conversion, re-entrant acquisition,
    /// fairness, waiter promotion, and deadlock handling are handled or deferred
    /// by the acquire/release layer rather than by this matrix.
    ///
    /// V0 uses conservative schema/data semantics:
    /// - [`LockMode::Exclusive`] conflicts with every other holder, including
    ///   another `Exclusive`, until a later same-transaction acquire rule can
    ///   safely override that at a higher layer.
    /// - [`LockMode::SchemaExclusive`] conflicts with every mode.
    /// - [`LockMode::Shared`] is compatible with `Shared`, `IntentShared`, and
    ///   `SchemaShared`; it conflicts with data/schema exclusivity intent.
    /// - Intent modes coordinate multi-granularity locking: `IntentShared` can
    ///   coexist with shared readers and both intent modes, while
    ///   `IntentExclusive` coexists only with intent/schema-stability holders
    ///   and does not coexist with data `Shared` or `Exclusive` holders on the
    ///   same resource.
    /// - [`LockMode::SchemaShared`] is a schema-stability mode that is broadly
    ///   compatible with non-exclusive data and intent work, but not with
    ///   `Exclusive` or `SchemaExclusive`.
    pub const fn is_compatible_with(self, requested: LockMode) -> bool {
        use LockMode::{IntentExclusive, IntentShared, SchemaShared, Shared};

        matches!(
            (self, requested),
            (Shared, Shared)
                | (Shared, IntentShared)
                | (Shared, SchemaShared)
                | (IntentShared, Shared)
                | (IntentShared, IntentShared)
                | (IntentShared, IntentExclusive)
                | (IntentShared, SchemaShared)
                | (IntentExclusive, IntentShared)
                | (IntentExclusive, IntentExclusive)
                | (IntentExclusive, SchemaShared)
                | (SchemaShared, Shared)
                | (SchemaShared, IntentShared)
                | (SchemaShared, IntentExclusive)
                | (SchemaShared, SchemaShared)
        )
    }
}

pub(super) fn held_mode_covers_requested(held_mode: LockMode, requested_mode: LockMode) -> bool {
    matches!(
        (held_mode, requested_mode),
        (LockMode::Exclusive, LockMode::Shared)
            | (LockMode::Exclusive, LockMode::IntentShared)
            | (LockMode::Exclusive, LockMode::IntentExclusive)
            | (LockMode::IntentExclusive, LockMode::IntentShared)
            | (LockMode::SchemaExclusive, LockMode::SchemaShared)
    )
}

pub(super) fn is_v0_upgrade(held_mode: LockMode, requested_mode: LockMode) -> bool {
    matches!(
        (held_mode, requested_mode),
        (LockMode::Shared, LockMode::Exclusive)
            | (LockMode::IntentShared, LockMode::IntentExclusive)
            | (LockMode::SchemaShared, LockMode::SchemaExclusive)
    )
}
