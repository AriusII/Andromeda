#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda StructuredObject Model

Contract-safe StructuredObject headers, layout descriptors, row-count
metadata policy, and deterministic descriptor hashing.

This crate owns the typed metadata model used before StructuredObject payload
bytes are admitted. It does not own Protobuf schemas, generated wire types,
transport framing, execution queues, catalog storage, WAL codecs, or payload
serialization.
"#]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{ColumnDescriptor, ContractHash};

mod hash;
mod shape;

#[cfg(test)]
mod tests;

/// Validate the boundary shape for a catalog StructuredObject definition.
///
/// This validation is runtime-free and covers only shape evidence:
/// field descriptors, ordinal/name constraints, and `unique_by` references.
pub fn validate_structured_object_shape(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> AndromedaResult<()> {
    shape::validate_structured_object_shape(fields, unique_by)
}

/// Encode canonical hash material for a StructuredObject shape boundary.
///
/// The material format is deterministic and independent from Rust native
/// struct layout so hash compatibility remains explicit and stable.
pub fn encode_structured_object_shape_material(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> Vec<u8> {
    shape::encode_structured_object_shape_material(fields, unique_by)
}

/// Compute deterministic hash for a StructuredObject shape boundary.
///
/// The hash excludes catalog object identity so compatibility is explicit:
/// equal shape hash means equal field + unique-key boundary material.
pub fn compute_structured_object_shape_hash(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> ContractHash {
    shape::compute_structured_object_shape_hash(fields, unique_by)
}

/// Explicit StructuredObject shape-hash compatibility gate.
///
/// Compatibility is exact-hash only: drift is incompatible.
pub fn structured_object_shape_hash_compatible(previous: ContractHash, next: ContractHash) -> bool {
    previous == next
}

/// Physical layout kind of a StructuredObject payload.
///
/// The layout participates in the descriptor hash so that two
/// otherwise-identical headers with different layouts produce distinct
/// fingerprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredObjectLayout {
    RowMajor,
    ColumnMajor,
    Hybrid,
}

impl StructuredObjectLayout {
    pub(crate) const fn hash_tag(self) -> u8 {
        match self {
            Self::RowMajor => 1,
            Self::ColumnMajor => 2,
            Self::Hybrid => 3,
        }
    }
}

/// Row-count policy carried by metadata before any result payload bytes.
///
/// The discriminants are stable manifest-hash tags for handwritten Rust
/// descriptors. They are not Protobuf wire numbers; generated Protobuf enums
/// remain governed by `contract.proto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCountRequirement {
    UnknownAllowed = 0,
    ExactIfKnown = 1,
    ExactRequired = 2,
}

/// Row-count policy carried by a StructuredObject contract header.
///
/// `RowCountPolicy` aliases [`RowCountRequirement`] so that the
/// StructuredObject contract surface and the result-stream descriptor surface
/// speak the same row-count vocabulary. This enforces the doctrinal rule that
/// exact row counts ride in metadata, before any payload bytes, rather than
/// being derived from a partial scan of the payload.
pub type RowCountPolicy = RowCountRequirement;

/// V0 StructuredObject contract header.
///
/// The header is the metadata-before-payload contract: every field describes
/// the StructuredObject identity, shape, and bounds before a single payload
/// byte is admitted. Payload framers (RPC batch, manifest emission, executor
/// egress) MUST validate this header before they accept or forward the
/// trailing `payload_length` bytes.
///
/// Field roles:
/// - `name`: stable, non-empty StructuredObject name (caller-provided
///   identity, not used by `descriptor_hash` so identical shapes under
///   different names share a descriptor identity).
/// - `contract_hash`: 32-byte fingerprint of the owning procedure or
///   compositional contract; never zero.
/// - `descriptor_hash`: 32-byte fingerprint computed deterministically
///   from `fields + layout` (see [`Self::compute_descriptor_hash`]); never
///   zero, and required to match the recomputed descriptor hash.
/// - `fields`: the ordered field descriptors (dense ordinals, unique
///   names, validated scalar types).
/// - `column_count`: must equal `fields.len()` so consumers can validate
///   metadata sufficiency before reading the payload.
/// - `layout`: physical layout kind (row-major, column-major, hybrid).
/// - `row_count_policy` + `row_count_exact`: row-count contract;
///   `ExactRequired` forces `row_count_exact` to be `Some` so a
///   ContractHash-bound `RowCountExact` invariant is verifiable from
///   metadata alone.
/// - `payload_length` / `max_payload_length`: payload byte budget; the
///   contract rejects payloads exceeding the declared bound and rejects
///   non-empty payloads when the metadata declares zero rows.
/// - `payload_checksum`: optional content checksum, opaque to this
///   contract layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredObjectHeader {
    pub name: String,
    pub contract_hash: ContractHash,
    pub descriptor_hash: ContractHash,
    pub fields: Vec<ColumnDescriptor>,
    pub column_count: u32,
    pub layout: StructuredObjectLayout,
    pub row_count_policy: RowCountPolicy,
    pub row_count_exact: Option<u64>,
    pub payload_length: u64,
    pub payload_checksum: Option<u64>,
    pub max_payload_length: Option<u64>,
}

impl StructuredObjectHeader {
    /// Compute the deterministic descriptor hash for the supplied field set
    /// and layout. The hash is computed without the StructuredObject name, so
    /// two headers with identical shapes under different names share a
    /// descriptor identity.
    pub fn compute_descriptor_hash(
        fields: &[ColumnDescriptor],
        layout: StructuredObjectLayout,
    ) -> ContractHash {
        hash::compute_descriptor_hash(fields, layout)
    }

    /// Validate every invariant that must hold on a StructuredObject contract
    /// header before its payload is accepted. This is the metadata-before-
    /// payload gate.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject name must not be empty",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject contract hash must not be zero",
            ));
        }

        if self.descriptor_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject descriptor hash must not be zero",
            ));
        }

        if self.fields.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject must declare at least one field",
            ));
        }

        if self.column_count as usize != self.fields.len() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject column_count must equal fields length",
            ));
        }

        // Validate field descriptors: per-column shape, dense ordinals, and
        // unique names. These checks fail Contract because they describe the
        // schema, not the wire payload.
        for (expected_ordinal, field) in self.fields.iter().enumerate() {
            field.validate()?;
            if field.ordinal as usize != expected_ordinal {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "StructuredObject field ordinals must be dense and zero-based",
                ));
            }
            if self
                .fields
                .iter()
                .filter(|other| other.name == field.name)
                .count()
                > 1
            {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "StructuredObject field names must be unique",
                ));
            }
        }

        // Descriptor hash must match the deterministic recomputation: this is
        // what makes the descriptor identity tamper-evident.
        let recomputed = Self::compute_descriptor_hash(&self.fields, self.layout);
        if recomputed != self.descriptor_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject descriptor hash does not match descriptor + layout",
            ));
        }

        // Row-count policy: ExactRequired forces an exact value so the
        // RowCountExact invariant is verifiable purely from metadata.
        if self.row_count_policy == RowCountPolicy::ExactRequired && self.row_count_exact.is_none()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "StructuredObject ExactRequired policy requires row_count_exact",
            ));
        }

        // Metadata-before-payload coherence: a header that declares zero rows
        // must not advertise a non-empty payload, and vice versa for strictly
        // positive declared row counts. These rules let downstream framers
        // reject inconsistent envelopes before reading payload bytes.
        if let Some(row_count_exact) = self.row_count_exact {
            if row_count_exact == 0 && self.payload_length != 0 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "StructuredObject declares zero rows but a non-empty payload",
                ));
            }
            if row_count_exact > 0 && self.payload_length == 0 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "StructuredObject declares positive rows but an empty payload",
                ));
            }
        }

        if let Some(max_payload_length) = self.max_payload_length
            && self.payload_length > max_payload_length
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "StructuredObject payload length exceeds declared bound",
            ));
        }

        Ok(())
    }
}
