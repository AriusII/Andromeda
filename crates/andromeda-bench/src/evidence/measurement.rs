#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkMeasurementMode {
    SyntheticDiagnostic,
    HarnessDiagnostic,
}

impl BenchmarkMeasurementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SyntheticDiagnostic => "synthetic-diagnostic",
            Self::HarnessDiagnostic => "harness-diagnostic",
        }
    }
}
