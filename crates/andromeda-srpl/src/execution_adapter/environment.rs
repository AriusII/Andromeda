use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

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
    /// Returns `None` only when the input is not bound; callers must not
    /// reinterpret absence as [`SrplBoundValue::Null`].
    fn get_input(&self, name: &str) -> Option<SrplBoundValue>;

    /// Retrieve a field value from a bound row.
    /// Returns `None` only when the binding, row, or field does not exist;
    /// callers must not reinterpret absence as [`SrplBoundValue::Null`].
    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue>;

    /// Retrieve the number of rows in a binding.
    /// Returns `None` if the binding does not exist.
    fn binding_row_count(&self, binding: &str) -> Option<usize>;

    /// Retrieve a required input and reject implicit Null semantics.
    fn get_required_input(&self, name: &str) -> AndromedaResult<SrplBoundValue> {
        self.get_input(name)
            .ok_or_else(|| missing_bound_value("SRPL binding environment input is not bound"))?
            .reject_implicit_null("SRPL binding environment input")
    }

    /// Retrieve a required field and reject implicit Null semantics.
    fn get_required_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> AndromedaResult<SrplBoundValue> {
        self.get_field_from_binding(binding, row_index, field)
            .ok_or_else(|| missing_bound_value("SRPL binding environment field is not bound"))?
            .reject_implicit_null("SRPL binding environment field")
    }
}

/// Field value carried by the binding environment for predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SrplBoundValue {
    Integer(i64),
    String(String),
    Bool(bool),
    /// Explicit catalog-declared Null value at the adapter edge.
    ///
    /// Missing bindings must be returned as `None`, not synthesized as Null.
    /// SRPL core values should call [`SrplBoundValue::reject_implicit_null`]
    /// unless the cataloged Procedure contract explicitly permits optional
    /// absence for that boundary.
    Null,
}

impl SrplBoundValue {
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn reject_implicit_null(self, context: &str) -> AndromedaResult<Self> {
        if matches!(self, Self::Null) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("{context} does not accept implicit Null"),
            ));
        }
        Ok(self)
    }
}

fn missing_bound_value(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
}

/// Empty trait for future row-type extension through the binding environment.
pub trait SrplBoundRow: Send + Sync {}
