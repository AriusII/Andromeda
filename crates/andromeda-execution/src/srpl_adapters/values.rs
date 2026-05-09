use std::collections::BTreeMap;

use andromeda_srpl_execution_adapter::SrplBoundValue;

/// Structured row object used by the SRPL execution adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructuredObject {
    pub fields: BTreeMap<String, FieldValue>,
}

impl StructuredObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_field(mut self, name: impl Into<String>, value: FieldValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    pub fn get_field(&self, name: &str) -> Option<&FieldValue> {
        self.fields.get(name)
    }
}

/// Typed field value supporting predicate evaluation and emit operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Integer(i64),
    String(String),
    Bool(bool),
    Null,
}

impl FieldValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn to_srpl_bound_value(&self) -> SrplBoundValue {
        match self {
            Self::Integer(v) => SrplBoundValue::Integer(*v),
            Self::String(s) => SrplBoundValue::String(s.clone()),
            Self::Bool(b) => SrplBoundValue::Bool(*b),
            Self::Null => SrplBoundValue::Null,
        }
    }

    pub fn from_srpl_bound_value(value: SrplBoundValue) -> Self {
        match value {
            SrplBoundValue::Integer(v) => Self::Integer(v),
            SrplBoundValue::String(s) => Self::String(s),
            SrplBoundValue::Bool(b) => Self::Bool(b),
            SrplBoundValue::Null => Self::Null,
        }
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        Self::Integer(v)
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
