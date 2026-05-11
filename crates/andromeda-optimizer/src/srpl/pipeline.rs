use andromeda_decision_trace::{PlanAlternativeCost, PlanAlternativeEvidence, PlanRejectionReason};
use andromeda_error::AndromedaResult;
use andromeda_srpl_ir::{SrplBusinessOperationKindIr, SrplProcedureBodyIr, SrplProcedureIr};

use super::{
    constant_fold::{fold_assignments, fold_emit_values},
    cost_model::{self, CostEstimate},
    diagnostics::{
        OptimizerDecisionKind, OptimizerDiagnostic, constant_folding_diagnostics,
        normalize_diagnostics, predicate_pushdown_diagnostics, projection_diagnostics,
        skipped_pass_diagnostic,
    },
    normalize,
    phase::OptimizerPhase,
    plan_choice::{self, AlternativePlanRecord},
    plan_kind::OptimizerPlanKind,
    predicate_pushdown,
    projection_pushdown::{self, EffectiveProjection},
    safety::ensure_effect_surface_preserved,
};

/// Conservative optimizer level for V1 compilation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// Preserve IR shape and run only cost/choice/projection evidence.
    None,
    /// Apply only proven semantics-preserving local rewrites.
    #[default]
    Safe,
    /// Reserved for future evidence-backed rewrites; currently aliases `Safe`.
    AggressiveEvidenceOnly,
}

/// Public optimizer pipeline configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizerPipelineConfig {
    /// Requested rewrite/evidence level.
    pub level: OptimizationLevel,
    /// Include explicit skipped-pass diagnostics for disabled rewrites.
    pub record_noop_decisions: bool,
}

impl OptimizerPipelineConfig {
    /// Build a config for `level` using production diagnostic defaults.
    pub const fn new(level: OptimizationLevel) -> Self {
        Self {
            level,
            record_noop_decisions: false,
        }
    }

    /// Enable explicit diagnostics for passes skipped by configuration.
    pub const fn with_noop_decisions(mut self, enabled: bool) -> Self {
        self.record_noop_decisions = enabled;
        self
    }
}

impl Default for OptimizerPipelineConfig {
    fn default() -> Self {
        Self::new(OptimizationLevel::Safe)
    }
}

/// Public optimizer pipeline output.
#[derive(Debug, Clone)]
pub struct OptimizerPipelineResult {
    /// Input IR before optimizer rewrites.
    pub original_ir: SrplProcedureIr,
    /// Chosen optimized IR.
    pub optimized_ir: SrplProcedureIr,
    /// Effective projections computed for read operations.
    pub projections: Vec<EffectiveProjection>,
    /// Cost estimate for the chosen plan.
    pub chosen_cost: CostEstimate,
    /// Chosen internal plan kind.
    pub chosen_kind: OptimizerPlanKind,
    /// Stable optimizer alternatives evidence, including the chosen plan.
    pub alternatives: Vec<AlternativePlanRecord>,
    /// Per-alternative cost breakdown in `DecisionTrace`-compatible form.
    ///
    /// Each entry mirrors the corresponding entry in `alternatives` but uses
    /// types from `andromeda-decision-trace` so callers can attach this
    /// breakdown to any `DecisionTrace` without depending on optimizer internals.
    pub alternative_cost_breakdown: Vec<PlanAlternativeEvidence>,
    /// Pipeline phases actually executed, in order.
    pub phases: Vec<OptimizerPhase>,
    /// Structured optimizer decisions and provenance emitted by the pipeline.
    pub diagnostics: Vec<OptimizerDiagnostic>,
}

/// Run the default safe optimizer pipeline.
pub fn run_optimizer_pipeline(ir: SrplProcedureIr) -> AndromedaResult<OptimizerPipelineResult> {
    optimize_procedure_ir_with_config(ir, OptimizerPipelineConfig::default())
}

/// Optimize a bounded procedure IR at the requested level.
pub fn optimize_procedure_ir(
    ir: SrplProcedureIr,
    level: OptimizationLevel,
) -> AndromedaResult<OptimizerPipelineResult> {
    optimize_procedure_ir_with_config(ir, OptimizerPipelineConfig::new(level))
}

/// Optimize a bounded procedure IR using an explicit pipeline config.
pub fn optimize_procedure_ir_with_config(
    ir: SrplProcedureIr,
    config: OptimizerPipelineConfig,
) -> AndromedaResult<OptimizerPipelineResult> {
    ir.body.validate_bounded()?;

    let original_ir = ir.clone();
    let mut phases = Vec::new();
    let mut diagnostics = Vec::new();
    let rewrite_enabled = matches!(
        config.level,
        OptimizationLevel::Safe | OptimizationLevel::AggressiveEvidenceOnly
    );

    let optimized = if rewrite_enabled {
        phases.push(OptimizerPhase::ConstantFolding);
        let folded = fold_constants_in_ir(ir.clone())?;
        diagnostics.extend(constant_folding_diagnostics(&ir, &folded));
        ensure_effect_surface_preserved(&original_ir, &folded, "ConstantFolding")?;

        phases.push(OptimizerPhase::PredicatePushdown);
        let pushed = predicate_pushdown::apply(folded.clone())?;
        diagnostics.extend(predicate_pushdown_diagnostics(&folded, &pushed));
        ensure_effect_surface_preserved(&original_ir, &pushed, "PredicatePushdown")?;

        phases.push(OptimizerPhase::Normalize);
        let normalized = normalize::normalize(pushed.clone())?;
        diagnostics.extend(normalize_diagnostics(&pushed, &normalized));
        ensure_effect_surface_preserved(&original_ir, &normalized, "Normalize")?;
        normalized
    } else {
        if config.record_noop_decisions {
            diagnostics.extend(
                OptimizerPhase::REWRITE_PHASES
                    .into_iter()
                    .map(|phase| skipped_pass_diagnostic(phase, config.level)),
            );
        }
        ir
    };

    phases.push(OptimizerPhase::ProjectionPushdown);
    let projection_result = projection_pushdown::apply(optimized);
    diagnostics.extend(projection_diagnostics(
        &projection_result.ir,
        &projection_result.projections,
    ));

    phases.push(OptimizerPhase::CostAnalysis);
    let cost = cost_model::estimate_without_stats(&projection_result.ir);
    debug_assert!(cost.is_valid());

    phases.push(OptimizerPhase::PlanChoice);
    let kind = OptimizerPlanKind::classify(&projection_result.ir);
    let choice = plan_choice::choose(vec![(projection_result.ir.clone(), cost, kind)])?;
    diagnostics.push(OptimizerDiagnostic::new(
        OptimizerPhase::PlanChoice,
        OptimizerDecisionKind::PlanChosen,
        None,
        None,
        format!("selected {:?} plan from {} alternative(s)", kind, 1),
    ));

    // Convert alternatives to DecisionTrace-compatible evidence at the boundary.
    let alternative_cost_breakdown = build_cost_breakdown(&choice.all_alternatives);

    Ok(OptimizerPipelineResult {
        original_ir,
        optimized_ir: choice.chosen_ir,
        projections: projection_result.projections,
        chosen_cost: choice.chosen_cost,
        chosen_kind: choice.chosen_kind,
        alternatives: choice.all_alternatives,
        alternative_cost_breakdown,
        phases,
        diagnostics,
    })
}

fn fold_constants_in_ir(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
    let mut operations = Vec::with_capacity(ir.body.operations.len());

    for mut operation in ir.body.operations {
        match &mut operation.kind {
            SrplBusinessOperationKindIr::Update { assignments, .. } => {
                *assignments = fold_assignments(std::mem::take(assignments));
            },
            SrplBusinessOperationKindIr::Emit { values, .. } => {
                *values = fold_emit_values(std::mem::take(values));
            },
            SrplBusinessOperationKindIr::Read { .. }
            | SrplBusinessOperationKindIr::Assert { .. }
            | SrplBusinessOperationKindIr::Raise { .. } => {},
        }
        operations.push(operation);
    }

    ir.body = SrplProcedureBodyIr { operations };
    ir.body.validate_bounded()?;
    Ok(ir)
}

/// Convert optimizer-internal `AlternativePlanRecord`s into
/// `PlanAlternativeEvidence` records suitable for a `DecisionTrace`.
///
/// This conversion happens at the optimizer boundary so that
/// `andromeda-decision-trace` remains free of any dependency on
/// `andromeda-optimizer`.  Invalid cost evidence is preserved as-is
/// (field values may be NaN / infinite) but the rejection reason is
/// mapped explicitly.
fn build_cost_breakdown(
    alternatives: &[plan_choice::AlternativePlanRecord],
) -> Vec<PlanAlternativeEvidence> {
    alternatives
        .iter()
        .filter_map(|alt| {
            let rejection = alt.rejection.map(|r| match r {
                plan_choice::RejectionReason::HigherCost => PlanRejectionReason::HigherCost,
                plan_choice::RejectionReason::TiedCostLowerPriority => {
                    PlanRejectionReason::TiedCostLowerPriority
                },
                plan_choice::RejectionReason::InvalidCostEvidence => {
                    PlanRejectionReason::InvalidCostEvidence
                },
            });
            let cost = PlanAlternativeCost {
                cpu_cost: alt.cost_estimate.cpu_cost,
                logical_io_cost: alt.cost_estimate.logical_io_cost,
                physical_io_cost: alt.cost_estimate.physical_io_cost,
                wal_cost: alt.cost_estimate.wal_cost,
                temp_cost: alt.cost_estimate.temp_cost,
                network_cost: alt.cost_estimate.network_cost,
                risk_penalty_cost: alt.cost_estimate.risk_penalty_cost,
                total_cost: alt.cost_estimate.total_cost,
            };
            // plan_kind labels are static strings; construction can only fail if the
            // label is empty or too long, neither of which applies to our static strs.
            PlanAlternativeEvidence::new(alt.plan_kind.as_str(), cost, alt.chosen, rejection).ok()
        })
        .collect()
}
