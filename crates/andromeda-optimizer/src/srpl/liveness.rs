use std::collections::BTreeSet;

use andromeda_srpl_ir::{
    SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureIr, SrplValueIr,
};

type ColRef = (String, String); // (binding, field)

/// Column liveness map, keyed by operation ordinal.
#[derive(Debug, Clone)]
pub struct ColumnLiveness {
    /// `live_after[i]` = set of (binding, field) live *after* operation `i`.
    live_after: Vec<BTreeSet<ColRef>>,
}

impl ColumnLiveness {
    /// Compute column liveness for the full procedure IR.
    ///
    /// Uses a single backward pass. The stored value is the set live after
    /// the current operation; the transfer function then derives the set
    /// live before it for the next earlier operation.
    pub fn compute(ir: &SrplProcedureIr) -> Self {
        let n = ir.body.operations.len();
        if n == 0 {
            return Self { live_after: vec![] };
        }

        let mut live_after: Vec<BTreeSet<ColRef>> = vec![BTreeSet::new(); n];
        let mut live: BTreeSet<ColRef> = BTreeSet::new();

        for i in (0..n).rev() {
            let op = &ir.body.operations[i];
            live_after[i] = live.clone();

            for def in compute_def(&op.kind) {
                live.retain(|(binding, _)| binding != &def);
            }
            live.extend(compute_use(&op.kind));
        }

        Self { live_after }
    }

    /// True when `(binding, field)` is live **after** the operation at `ordinal`.
    /// I.e., at least one downstream operation uses it.
    pub fn is_live_after(&self, ordinal: u32, binding: &str, field: &str) -> bool {
        let idx = ordinal as usize;
        if idx >= self.live_after.len() {
            return false;
        }
        self.live_after[idx].contains(&(binding.to_string(), field.to_string()))
    }

    /// Collect all `(binding, field)` pairs live after operation `ordinal`.
    pub fn live_columns_after(&self, ordinal: u32) -> Vec<(String, String)> {
        let idx = ordinal as usize;
        if idx >= self.live_after.len() {
            return vec![];
        }
        self.live_after[idx].iter().cloned().collect()
    }
}

/// USE set for an operation — all (binding, field) pairs it reads.
fn compute_use(kind: &SrplBusinessOperationKindIr) -> BTreeSet<ColRef> {
    let mut set = BTreeSet::new();
    match kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            for p in predicates {
                add_predicate_use(&mut set, p);
            }
        }
        SrplBusinessOperationKindIr::Assert { predicate, .. } => {
            add_predicate_use(&mut set, predicate);
        }
        SrplBusinessOperationKindIr::Update {
            predicates,
            assignments,
            ..
        } => {
            for p in predicates {
                add_predicate_use(&mut set, p);
            }
            for a in assignments {
                add_value_use(&mut set, &a.value);
            }
        }
        SrplBusinessOperationKindIr::Emit { values, .. } => {
            for ev in values {
                add_value_use(&mut set, &ev.value);
            }
        }
        SrplBusinessOperationKindIr::Raise { .. } => {}
    }
    set
}

fn add_predicate_use(set: &mut BTreeSet<ColRef>, pred: &SrplPredicateIr) {
    match pred {
        SrplPredicateIr::InputEqualsField { binding, field, .. }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, field, .. } => {
            set.insert((binding.clone(), field.clone()));
        }
    }
}

fn add_value_use(set: &mut BTreeSet<ColRef>, value: &SrplValueIr) {
    match value {
        SrplValueIr::Field { binding, field } => {
            set.insert((binding.clone(), field.clone()));
        }
        SrplValueIr::SubtractInput { binding, field, .. } => {
            set.insert((binding.clone(), field.clone()));
        }
        SrplValueIr::BinaryArith { left, right, .. } => {
            add_value_use(set, left);
            add_value_use(set, right);
        }
        SrplValueIr::Input(_) | SrplValueIr::Bool(_) | SrplValueIr::Constant(_) => {}
    }
}

/// DEF set for an operation — binding names it introduces (shadowing
/// any previous live columns for that binding).
fn compute_def(kind: &SrplBusinessOperationKindIr) -> Vec<String> {
    match kind {
        SrplBusinessOperationKindIr::Read { binding, .. } => vec![binding.clone()],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::QualifiedName;
    use andromeda_srpl_ir::{
        Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
        SrplProcedureIr,
    };
    use andromeda_srpl_ir::{SrplEmitValueIr, SrplPredicateIr, SrplValueIr};

    fn qn(s: &str) -> QualifiedName {
        QualifiedName::parse(s).unwrap()
    }

    fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
        SrplProcedureIr {
            name: qn("db.ns.P"),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr { operations: ops },
        }
    }

    fn read_op(
        ord: u32,
        binding: &str,
        predicates: Vec<SrplPredicateIr>,
    ) -> SrplBusinessOperationIr {
        SrplBusinessOperationIr {
            ordinal: ord,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("db.ns.T"),
                binding: binding.into(),
                cardinality: Cardinality::One,
                predicates,
            },
        }
    }

    fn emit_op(ord: u32, binding: &str, field: &str) -> SrplBusinessOperationIr {
        SrplBusinessOperationIr {
            ordinal: ord,
            kind: SrplBusinessOperationKindIr::Emit {
                stream: "S".into(),
                values: vec![SrplEmitValueIr {
                    column: "out".into(),
                    value: SrplValueIr::Field {
                        binding: binding.into(),
                        field: field.into(),
                    },
                }],
            },
        }
    }

    // T-PJ-01 equivalent: only 'id' is live (used in emit), 'name' and 'qty' are dead.
    #[test]
    fn only_emitted_column_is_live() {
        let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
        let liveness = ColumnLiveness::compute(&ir);
        assert!(
            liveness.is_live_after(0, "T", "id"),
            "id must be live after Read"
        );
        assert!(
            !liveness.is_live_after(0, "T", "name"),
            "name must be dead after Read"
        );
        assert!(
            !liveness.is_live_after(0, "T", "qty"),
            "qty must be dead after Read"
        );
    }

    // T-PJ-02 equivalent: assert uses A.status, emit uses A.id → both live.
    #[test]
    fn assert_and_emit_columns_are_live() {
        let assert_op = SrplBusinessOperationIr {
            ordinal: 1,
            kind: SrplBusinessOperationKindIr::Assert {
                predicate: SrplPredicateIr::InputEqualsField {
                    input: "s".into(),
                    binding: "T".into(),
                    field: "status".into(),
                },
                failure_code: "ERR".into(),
            },
        };
        let ir = make_ir(vec![
            read_op(0, "T", vec![]),
            assert_op,
            emit_op(2, "T", "id"),
        ]);
        let liveness = ColumnLiveness::compute(&ir);
        assert!(liveness.is_live_after(0, "T", "id"), "id must be live");
        assert!(
            liveness.is_live_after(0, "T", "status"),
            "status must be live"
        );
    }

    #[test]
    fn liveness_is_empty_for_empty_body() {
        let ir = make_ir(vec![]);
        let liveness = ColumnLiveness::compute(&ir);
        assert!(!liveness.is_live_after(0, "T", "id"));
    }
}
