use std::collections::BTreeSet;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor, digest::Sha256,
};

use super::procedure_manifest::write_tagged;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub cardinality: ResultCardinality,
    pub row_count_requirement: RowCountRequirement,
    pub row_count_exact: Option<u64>,
    /// Optional inclusive upper bound on the number of rows the stream may
    /// emit. Used to declare a *bounded* `ZeroOrMore` / `OneOrMore` result
    /// without inventing a streaming engine. When present, must respect the
    /// declared cardinality (intrinsic max of 1 for `ZeroOrOne`/`ExactlyOne`,
    /// minimum of 1 for `OneOrMore`) and must be `>= row_count_exact` when
    /// both are declared.
    pub row_count_max: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultCardinality {
    ZeroOrMore,
    ZeroOrOne,
    OneOrMore,
    ExactlyOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCountRequirement {
    UnknownAllowed,
    ExactIfKnown,
    ExactRequired,
}

impl ResultCardinality {
    /// Lower bound on the number of rows a stream of this cardinality may
    /// emit.
    pub const fn min_row_count(self) -> u64 {
        match self {
            Self::ExactlyOne | Self::OneOrMore => 1,
            Self::ZeroOrOne | Self::ZeroOrMore => 0,
        }
    }

    /// Intrinsic upper bound carried by the cardinality kind itself.
    /// `ExactlyOne` and `ZeroOrOne` are intrinsically bounded at 1.
    pub const fn intrinsic_max_row_count(self) -> Option<u64> {
        match self {
            Self::ExactlyOne | Self::ZeroOrOne => Some(1),
            Self::OneOrMore | Self::ZeroOrMore => None,
        }
    }

    /// True when an explicit upper bound is consistent with this cardinality
    /// (i.e. it neither contradicts the intrinsic max nor the minimum row
    /// count).
    pub const fn permits_row_count_max(self, row_count_max: u64) -> bool {
        if row_count_max < self.min_row_count() {
            return false;
        }
        match self.intrinsic_max_row_count() {
            Some(intrinsic) => row_count_max <= intrinsic,
            None => true,
        }
    }
}

impl ResultStreamDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.stream_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream descriptor name must not be empty",
            ));
        }

        if self.columns.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream descriptor requires at least one typed column",
            ));
        }

        let mut seen_column_names = BTreeSet::new();
        let mut seen_column_ordinals = BTreeSet::new();
        for column in &self.columns {
            column.validate()?;
            if !seen_column_names.insert(column.name.clone()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream descriptor column names must be unique",
                ));
            }
            if !seen_column_ordinals.insert(column.ordinal) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream descriptor column ordinals must be unique",
                ));
            }
        }

        for expected in 0..self.columns.len() as u32 {
            if !seen_column_ordinals.contains(&expected) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream descriptor column ordinals must be dense and zero-based",
                ));
            }
        }

        if self.row_count_requirement == RowCountRequirement::ExactRequired
            && self.row_count_exact.is_none()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream requires an exact row count",
            ));
        }

        if let Some(row_count_exact) = self.row_count_exact {
            match self.cardinality {
                ResultCardinality::ZeroOrMore => {}
                ResultCardinality::ZeroOrOne if row_count_exact <= 1 => {}
                ResultCardinality::OneOrMore if row_count_exact >= 1 => {}
                ResultCardinality::ExactlyOne if row_count_exact == 1 => {}
                _ => {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "exact row count violates result stream cardinality",
                    ));
                }
            }
        }

        if let Some(row_count_max) = self.row_count_max {
            if !self.cardinality.permits_row_count_max(row_count_max) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream row_count_max violates cardinality bounds",
                ));
            }
            if let Some(row_count_exact) = self.row_count_exact
                && row_count_exact > row_count_max
            {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream row_count_exact exceeds declared row_count_max",
                ));
            }
        }

        Ok(())
    }

    /// Fold the descriptor into a [`Sha256`] hasher in a length-prefixed,
    /// field-tagged canonical form. Used by [`ProcedureManifest::manifest_hash`]
    /// to keep the manifest digest deterministic across builds.
    pub(super) fn absorb_into(&self, hasher: &mut Sha256) {
        write_tagged(hasher, b"stream.name", self.stream_name.as_bytes());
        hasher.update(b"stream.columns:");
        hasher.update(&(self.columns.len() as u64).to_be_bytes());
        for column in &self.columns {
            write_tagged(hasher, b"column.name", column.name.as_bytes());
            hasher.update(b"column.ordinal:");
            hasher.update(&column.ordinal.to_be_bytes());
            // Debug formatting on TypeDescriptor is exhaustive over the enum
            // variants and stable for our purposes; any change to the
            // descriptor shape is a deliberate wire break and is allowed to
            // change the manifest digest.
            let formatted = format!("{:?}", column.data_type);
            write_tagged(hasher, b"column.type", formatted.as_bytes());
        }
        hasher.update(b"stream.cardinality:");
        hasher.update(&[self.cardinality as u8]);
        hasher.update(b"stream.row_count_requirement:");
        hasher.update(&[self.row_count_requirement as u8]);
        hasher.update(b"stream.row_count_exact:");
        match self.row_count_exact {
            Some(v) => {
                hasher.update(&[1]);
                hasher.update(&v.to_be_bytes());
            }
            None => hasher.update(&[0]),
        }
        hasher.update(b"stream.row_count_max:");
        match self.row_count_max {
            Some(v) => {
                hasher.update(&[1]);
                hasher.update(&v.to_be_bytes());
            }
            None => hasher.update(&[0]),
        }
    }
}
