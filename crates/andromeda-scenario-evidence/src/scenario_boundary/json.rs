use crate::flat_json::{escape_json_string, json_optional_str};

use super::evidence::BenchmarkScenarioEvidence;

pub(super) fn scenario_evidence_to_json(evidence: &BenchmarkScenarioEvidence) -> String {
    let target = evidence.target();
    let budgets = evidence.budgets();
    let confidence = evidence.confidence();
    let validity = evidence.validity();
    let context = evidence.context();

    format!(
        r#"{{"workload_id":"{}","commit_id":"{}","observed_at":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"target_procedure_id":{},"target_catalog_version":{},"target_contract_hash":"{}","target_stats_version":{},"target_plan_class":"{}","target_plan_class_tag":{},"duration_budget_ms":{},"sample_budget":{},"temp_budget_bytes":{},"confidence_permille":{},"issued_at_unix_ms":{},"expires_at_unix_ms":{},"workload_class":"{}","hardware_profile":{},"measurement_mode":{},"latency_source":{},"timing_source":{},"engine_harness":{},"synthetic_model_version":{},"authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}"}}"#,
        escape_json_string(evidence.workload_id()),
        escape_json_string(evidence.commit_id()),
        escape_json_string(evidence.observed_at()),
        evidence.p50_latency_us(),
        evidence.p95_latency_us(),
        evidence.error_count(),
        evidence.sample_count(),
        target.procedure_id.get(),
        target.catalog_version.get(),
        target.contract_hash,
        target.stats_version.get(),
        target.plan_class.as_str(),
        target.plan_class.as_tag(),
        budgets.duration_ms,
        budgets.samples,
        budgets.temp_bytes,
        confidence.permille(),
        validity.issued_at().as_unix_millis(),
        validity.expires_at().as_unix_millis(),
        context.workload_class().as_str(),
        json_optional_str(context.hardware_profile().map(|profile| profile.as_str())),
        json_optional_str(context.measurement_mode().map(|mode| mode.as_str())),
        json_optional_str(context.latency_source()),
        json_optional_str(context.timing_source()),
        json_optional_str(context.engine_harness()),
        json_optional_str(context.synthetic_model_version()),
        evidence.is_authoritative(),
        evidence.can_select_plan_alone(),
        escape_json_string(evidence.optimizer_consumption_role())
    )
}
