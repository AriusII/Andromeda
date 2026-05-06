use crate::ir::SrplPredicateIr;

/// Result of simplifying a predicate list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimplifiedPredicates {
    /// The list evaluates to `false` for every possible input.
    /// The caller should replace the enclosing operation with `Raise`.
    AlwaysFalse,
    /// A (possibly empty) simplified predicate list.
    /// Empty = no filter condition (full scan / no-op assert).
    Predicates(Vec<SrplPredicateIr>),
}

/// Canonical string key for deduplication.
fn predicate_key(pred: &SrplPredicateIr) -> String {
    match pred {
        SrplPredicateIr::InputEqualsField {
            input,
            binding,
            field,
        } => format!("EQ:{}:{}:{}", input, binding, field),
        SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding,
            field,
            input,
        } => format!("GTE:{}:{}:{}", binding, field, input),
    }
}

/// A predicate is a tautology when both sides reference the same symbol.
/// Currently detects only the degenerate case where `input == field` in
/// an `InputEqualsField` predicate with matching `binding` and `input`
/// names — an artefact of degenerate lowering.
fn is_tautology(_pred: &SrplPredicateIr) -> bool {
    // Conservative: no tautology is detectable without type-range analysis.
    false
}

/// A predicate is a contradiction when it can never be satisfied.
/// Currently not detectable without value knowledge; always returns false.
fn is_contradiction(_pred: &SrplPredicateIr) -> bool {
    false
}

/// Simplify a predicate list.
///
/// - Removes exact structural duplicates (deduplication).
/// - Removes tautological predicates (currently: none detected).
/// - Returns `AlwaysFalse` when any predicate is contradictory (currently: none).
/// - Never grows the list.
/// - Idempotent: `simplify(simplify(ps)) == simplify(ps)`.
pub fn simplify_predicates(predicates: Vec<SrplPredicateIr>) -> SimplifiedPredicates {
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();

    for pred in predicates {
        if is_contradiction(&pred) {
            return SimplifiedPredicates::AlwaysFalse;
        }
        if is_tautology(&pred) {
            continue;
        }
        let key = predicate_key(&pred);
        if seen.insert(key) {
            result.push(pred);
        }
    }

    SimplifiedPredicates::Predicates(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SrplPredicateIr;

    fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
        SrplPredicateIr::InputEqualsField {
            input: input.into(),
            binding: binding.into(),
            field: field.into(),
        }
    }

    // T-PF-01
    #[test]
    fn empty_list_returns_empty() {
        assert_eq!(
            simplify_predicates(vec![]),
            SimplifiedPredicates::Predicates(vec![])
        );
    }

    // T-PF-02
    #[test]
    fn duplicate_predicates_are_deduplicated() {
        let p = eq_pred("id", "T", "id");
        let result = simplify_predicates(vec![p.clone(), p.clone()]);
        assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
    }

    // T-PF-03
    #[test]
    fn distinct_predicates_are_preserved() {
        let p1 = eq_pred("id", "T", "id");
        let p2 = eq_pred("name", "T", "name");
        let p3 = eq_pred("qty", "T", "qty");
        let result = simplify_predicates(vec![p1.clone(), p2.clone(), p3.clone()]);
        assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2, p3]));
    }

    #[test]
    fn simplify_is_idempotent() {
        let p = eq_pred("id", "T", "id");
        let input = vec![p.clone(), p.clone()];
        let once = match simplify_predicates(input) {
            SimplifiedPredicates::Predicates(v) => v,
            SimplifiedPredicates::AlwaysFalse => vec![],
        };
        let twice = match simplify_predicates(once.clone()) {
            SimplifiedPredicates::Predicates(v) => v,
            SimplifiedPredicates::AlwaysFalse => vec![],
        };
        assert_eq!(once, twice);
    }

    #[test]
    fn result_never_grows() {
        let p1 = eq_pred("a", "T", "a");
        let p2 = eq_pred("b", "T", "b");
        let input = vec![p1.clone(), p2.clone(), p1.clone()];
        let result = match simplify_predicates(input.clone()) {
            SimplifiedPredicates::Predicates(v) => v,
            SimplifiedPredicates::AlwaysFalse => vec![],
        };
        assert!(result.len() <= input.len());
    }
}
