/// Trait defining the binding environment contract for SRPL predicate evaluation.
///
/// The binding environment provides deterministic, bounded access to:
/// - Input parameters from procedure inputs.
/// - Read bindings from prior READ operations.
/// - Local variables for future extension.
///
/// Adapters receive a read-only reference to this environment to evaluate
/// predicates without side effects or non-determinism.
pub trait SrplBindingEnvironment: Send + Sync {
    /// Retrieve an input parameter value by name.
    /// Returns None if the input is not bound.
    fn get_input(&self, name: &str) -> Option<SrplBoundValue>;

    /// Retrieve a field value from a bound row.
    /// Returns None if the binding or field does not exist.
    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue>;

    /// Retrieve the number of rows in a binding.
    /// Returns None if the binding does not exist.
    fn binding_row_count(&self, binding: &str) -> Option<usize>;
}

/// Field value carried by the binding environment for predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SrplBoundValue {
    Integer(i64),
    String(String),
    Bool(bool),
    Null,
}

/// Empty trait for future row-type extension through the binding environment.
pub trait SrplBoundRow: Send + Sync {}
