use std::collections::BTreeMap;

use andromeda_srpl::execution_adapter::{SrplBindingEnvironment, SrplBoundValue};

use super::values::{FieldValue, StructuredObject};

/// Scoped type environment for predicate binding.
#[derive(Debug, Clone, Default)]
pub struct SrplTypedEnvironment {
    bindings: BTreeMap<String, Vec<StructuredObject>>,
    inputs: BTreeMap<String, FieldValue>,
}

impl SrplTypedEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_input(&mut self, name: impl Into<String>, value: FieldValue) {
        self.inputs.insert(name.into(), value);
    }

    pub fn bind_read(&mut self, binding_name: impl Into<String>, rows: Vec<StructuredObject>) {
        self.bindings.insert(binding_name.into(), rows);
    }

    pub fn get_input_internal(&self, name: &str) -> Option<&FieldValue> {
        self.inputs.get(name)
    }

    pub fn get_binding_internal(&self, name: &str) -> Option<&Vec<StructuredObject>> {
        self.bindings.get(name)
    }
}

impl SrplBindingEnvironment for SrplTypedEnvironment {
    fn get_input(&self, name: &str) -> Option<SrplBoundValue> {
        self.inputs.get(name).map(|fv| fv.to_srpl_bound_value())
    }

    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue> {
        self.bindings
            .get(binding)
            .and_then(|rows| rows.get(row_index))
            .and_then(|row| row.get_field(field))
            .map(|fv| fv.to_srpl_bound_value())
    }

    fn binding_row_count(&self, binding: &str) -> Option<usize> {
        self.bindings.get(binding).map(|rows| rows.len())
    }
}
