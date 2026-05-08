#![forbid(unsafe_code)]

//! Vector advisory boundary.
//!
//! Vector outputs may rank or suggest analytical candidates. They do not become
//! durable truth, transaction truth, catalog truth, or security authority.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_hardware::PipelineClass;

/// Advisory vector workload class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorAdvisoryKind {
    SimilarityRanking,
    CandidatePruning,
    MapRefreshHint,
}

/// Request to accept vector output as advisory evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorAdvisoryRequest {
    pub pipeline: PipelineClass,
    pub kind: VectorAdvisoryKind,
    pub source_truth_available: bool,
    pub bounded_candidates: bool,
    pub advisory_only: bool,
}

impl VectorAdvisoryRequest {
    pub const fn new(pipeline: PipelineClass, kind: VectorAdvisoryKind) -> Self {
        Self {
            pipeline,
            kind,
            source_truth_available: false,
            bounded_candidates: false,
            advisory_only: false,
        }
    }

    pub const fn with_source_truth(mut self) -> Self {
        self.source_truth_available = true;
        self
    }

    pub const fn with_bounded_candidates(mut self) -> Self {
        self.bounded_candidates = true;
        self
    }

    pub const fn advisory_only(mut self) -> Self {
        self.advisory_only = true;
        self
    }
}

/// Decision emitted by the vector advisory boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorAdvisoryDecision {
    pub pipeline: PipelineClass,
    pub kind: VectorAdvisoryKind,
    pub advisory_only: bool,
    pub may_publish_as_truth: bool,
}

/// Validates vector output as advisory-only evidence.
pub fn validate_vector_advisory(
    request: VectorAdvisoryRequest,
) -> AndromedaResult<VectorAdvisoryDecision> {
    reject_c5_pipeline(request.pipeline, "vector")?;

    if !request.pipeline.is_optional_acceleration_candidate() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "vector advisory output is not permitted for {} pipeline",
                request.pipeline.name()
            ),
        ));
    }

    if !request.source_truth_available {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector advisory output requires independent source truth",
        ));
    }

    if !request.bounded_candidates {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector advisory output requires bounded candidates",
        ));
    }

    if !request.advisory_only {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector output must remain advisory-only",
        ));
    }

    Ok(VectorAdvisoryDecision {
        pipeline: request.pipeline,
        kind: request.kind,
        advisory_only: true,
        may_publish_as_truth: false,
    })
}

fn reject_c5_pipeline(pipeline: PipelineClass, accelerator: &str) -> AndromedaResult<()> {
    if pipeline.is_c5_truth_path() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "optional {accelerator} execution is forbidden on C5 {} pipeline",
                pipeline.name()
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c5_pipelines() -> [PipelineClass; 7] {
        [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::MvccVisibility,
            PipelineClass::CatalogPublication,
            PipelineClass::SecurityCriticalPath,
        ]
    }

    fn valid_request(pipeline: PipelineClass) -> VectorAdvisoryRequest {
        VectorAdvisoryRequest::new(pipeline, VectorAdvisoryKind::SimilarityRanking)
            .with_source_truth()
            .with_bounded_candidates()
            .advisory_only()
    }

    #[test]
    fn vector_advisory_rejects_every_c5_truth_path() {
        for pipeline in c5_pipelines() {
            let error = validate_vector_advisory(valid_request(pipeline)).unwrap_err();

            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(error.message().contains("C5"));
            assert!(error.message().contains(pipeline.name()));
        }
    }

    #[test]
    fn vector_advisory_rejects_non_advisory_non_c5_pipeline() {
        let error = validate_vector_advisory(valid_request(PipelineClass::ForegroundExecution))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("foreground_execution"));
    }

    #[test]
    fn vector_advisory_requires_independent_source_truth() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_bounded_candidates()
        .advisory_only();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("source truth"));
    }

    #[test]
    fn vector_advisory_requires_bounded_candidates() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_source_truth()
        .advisory_only();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("bounded candidates"));
    }

    #[test]
    fn vector_output_must_remain_advisory_only() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_source_truth()
        .with_bounded_candidates();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("advisory-only"));
    }

    #[test]
    fn vector_advisory_never_publishes_truth() {
        let decision =
            validate_vector_advisory(valid_request(PipelineClass::BatchAnalytics)).unwrap();

        assert!(decision.advisory_only);
        assert!(!decision.may_publish_as_truth);
    }
}
