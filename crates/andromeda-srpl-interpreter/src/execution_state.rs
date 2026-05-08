use andromeda_srpl_execution_adapter::{SrplBindingEnvironment, SrplBoundValue};

/// Aggregate deterministic execution counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SrplInterpreterReport {
    pub operations_executed: usize,
    pub reads: usize,
    pub assertions: usize,
    pub updates: usize,
    pub emits: usize,
}

/// Mutable state owned by a single deterministic interpreter invocation.
pub(super) struct SrplExecutionState {
    report: SrplInterpreterReport,
    environment: MinimalBindingEnvironment,
}

impl SrplExecutionState {
    pub(super) fn new() -> Self {
        Self {
            report: SrplInterpreterReport::default(),
            environment: MinimalBindingEnvironment,
        }
    }

    pub(super) fn environment(&self) -> &dyn SrplBindingEnvironment {
        &self.environment
    }

    pub(super) fn record_operation(&mut self) {
        self.report.operations_executed += 1;
    }

    pub(super) fn record_read(&mut self) {
        self.report.reads += 1;
    }

    pub(super) fn record_assertion(&mut self) {
        self.report.assertions += 1;
    }

    pub(super) fn record_update(&mut self) {
        self.report.updates += 1;
    }

    pub(super) fn record_emit(&mut self) {
        self.report.emits += 1;
    }

    pub(super) fn finish(self) -> SrplInterpreterReport {
        self.report
    }
}

/// Empty binding environment used by the deterministic interpreter.
///
/// The current narrow SRPL operation set validates symbol shapes and bounded
/// contracts but does not require dynamic input/binding value lookup at this
/// layer.
struct MinimalBindingEnvironment;

impl SrplBindingEnvironment for MinimalBindingEnvironment {
    fn get_input(&self, _name: &str) -> Option<SrplBoundValue> {
        None
    }

    fn get_field_from_binding(
        &self,
        _binding: &str,
        _row_index: usize,
        _field: &str,
    ) -> Option<SrplBoundValue> {
        None
    }

    fn binding_row_count(&self, _binding: &str) -> Option<usize> {
        None
    }
}
