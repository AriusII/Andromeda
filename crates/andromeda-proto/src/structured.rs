use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor, ContractHash,
    DecimalType, FloatMode, FloatType, ScalarType, TextEncoding, TimestampType, digest::Sha256,
};

use crate::manifest::RowCountRequirement;

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
    fn tag(self) -> u8 {
        match self {
            Self::RowMajor => 1,
            Self::ColumnMajor => 2,
            Self::Hybrid => 3,
        }
    }
}

/// Row-count policy carried by a StructuredObject contract header.
///
/// `RowCountPolicy` re-exports [`RowCountRequirement`] so that the
/// StructuredObject contract surface and the result-stream descriptor
/// surface speak the same row-count vocabulary.  This enforces the doctrinal
/// rule that exact row counts ride in metadata, *before* any payload bytes,
/// rather than being derived from a partial scan of the payload.
pub type RowCountPolicy = RowCountRequirement;

/// V0 StructuredObject contract header.
///
/// The header is the *metadata-before-payload* contract: every field
/// describes the StructuredObject identity, shape, and bounds before a
/// single payload byte is admitted.  Payload framers (RPC batch, manifest
/// emission, executor egress) MUST validate this header before they
/// accept or forward the trailing `payload_length` bytes.
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
    /// and layout.  The hash is computed *without* the StructuredObject
    /// name, so two headers with identical shapes under different names
    /// share a descriptor identity (descriptor / name separation).
    pub fn compute_descriptor_hash(
        fields: &[ColumnDescriptor],
        layout: StructuredObjectLayout,
    ) -> ContractHash {
        let mut sink = StructuredObjectHashSink::new();
        sink.tag("andromeda.structured-object.descriptor.v1.sha256");
        sink.layout(layout);
        sink.columns(fields);
        sink.finish()
    }

    /// Validate every invariant that must hold on a StructuredObject
    /// contract header *before* its payload is accepted.  This is the
    /// metadata-before-payload gate.
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

        // Validate field descriptors: per-column shape, dense ordinals,
        // and unique names.  These checks fail Contract because they
        // describe the schema, not the wire payload.
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

        // Descriptor hash must match the deterministic recomputation: this
        // is what makes the descriptor identity tamper-evident.
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

        // Metadata-before-payload coherence: a header that declares zero
        // rows must not advertise a non-empty payload, and vice versa for
        // strictly positive declared row counts.  These rules let
        // downstream framers reject inconsistent envelopes before reading
        // payload bytes.
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

        if let Some(max_payload_length) = self.max_payload_length {
            if self.payload_length > max_payload_length {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "StructuredObject payload length exceeds declared bound",
                ));
            }
        }

        Ok(())
    }
}

struct StructuredObjectHashSink {
    hasher: Sha256,
}

impl StructuredObjectHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> ContractHash {
        ContractHash::new(self.hasher.finalize())
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.raw(&[value]);
    }

    fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw(bytes);
    }

    fn tag(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn layout(&mut self, layout: StructuredObjectLayout) {
        self.u8(layout.tag());
    }

    fn columns(&mut self, columns: &[ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.bytes(column.name.as_bytes());
            self.u32(column.ordinal);
            self.scalar(&column.data_type.scalar);
            self.u8(match column.data_type.absence {
                andromeda_core::AbsencePolicy::Required => 0,
                andromeda_core::AbsencePolicy::ExplicitOptional => 1,
            });
        }
    }

    fn scalar(&mut self, scalar: &ScalarType) {
        match scalar {
            ScalarType::I8 => self.u8(0),
            ScalarType::I16 => self.u8(1),
            ScalarType::I32 => self.u8(2),
            ScalarType::I64 => self.u8(3),
            ScalarType::I128 => self.u8(4),
            ScalarType::U8 => self.u8(5),
            ScalarType::U16 => self.u8(6),
            ScalarType::U32 => self.u8(7),
            ScalarType::U64 => self.u8(8),
            ScalarType::U128 => self.u8(9),
            ScalarType::Decimal(decimal) => {
                self.u8(10);
                match decimal {
                    DecimalType::Min => self.u8(0),
                    DecimalType::Mid => self.u8(1),
                    DecimalType::Max => self.u8(2),
                    DecimalType::Custom { precision, scale } => {
                        self.u8(3);
                        self.u8(*precision);
                        self.u8(*scale);
                    }
                }
            }
            ScalarType::Float(float) => {
                self.u8(11);
                match float {
                    FloatType::Min => self.u8(0),
                    FloatType::Mid => self.u8(1),
                    FloatType::Max => self.u8(2),
                    FloatType::Custom { bits, mode } => {
                        self.u8(3);
                        self.raw(&bits.to_le_bytes());
                        self.u8(match mode {
                            FloatMode::Approximate => 0,
                            FloatMode::DeterministicAnalytics => 1,
                        });
                    }
                }
            }
            ScalarType::Bool => self.u8(12),
            ScalarType::Text(text) => {
                self.u8(13);
                self.u8(match text.encoding {
                    TextEncoding::Utf8 => 0,
                    TextEncoding::Utf16 => 1,
                    TextEncoding::Unicode => 2,
                });
                self.u32(text.max_length.unwrap_or(0));
                self.bool(text.max_length.is_some());
                match &text.collation {
                    Some(collation) => {
                        self.bool(true);
                        self.bytes(collation.as_bytes());
                    }
                    None => self.bool(false),
                }
            }
            ScalarType::Timestamp(timestamp) => {
                self.u8(14);
                self.u8(match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{ScalarType, TypeDescriptor};

    fn fields() -> Vec<ColumnDescriptor> {
        vec![
            ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            },
            ColumnDescriptor {
                name: "Quantity".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I32),
                ordinal: 1,
            },
        ]
    }

    fn header(name: &str) -> StructuredObjectHeader {
        let layout = StructuredObjectLayout::RowMajor;
        let fields = fields();
        let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
        StructuredObjectHeader {
            name: name.to_string(),
            contract_hash: ContractHash::test_vector(7),
            descriptor_hash,
            column_count: fields.len() as u32,
            fields,
            layout,
            row_count_policy: RowCountPolicy::ExactRequired,
            row_count_exact: Some(1),
            payload_length: 64,
            payload_checksum: Some(0xA5A5),
            max_payload_length: Some(256),
        }
    }

    #[test]
    fn descriptor_hash_is_deterministic_and_independent_of_name() {
        // Identical descriptor + layout under different names share an
        // identity: name is *not* in the descriptor hash.
        let a = header("Reservation");
        let b = header("OtherName");
        assert_eq!(a.descriptor_hash, b.descriptor_hash);

        // Two independent computations on the same field set/layout match.
        assert_eq!(
            StructuredObjectHeader::compute_descriptor_hash(
                &fields(),
                StructuredObjectLayout::RowMajor
            ),
            StructuredObjectHeader::compute_descriptor_hash(
                &fields(),
                StructuredObjectLayout::RowMajor
            )
        );
    }

    #[test]
    fn descriptor_hash_changes_when_layout_changes() {
        let row_major = StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::RowMajor,
        );
        let column_major = StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::ColumnMajor,
        );
        assert_ne!(row_major, column_major);
    }

    #[test]
    fn descriptor_hash_changes_when_field_set_changes() {
        let mut alt = fields();
        alt[1].name = "Renamed".to_string();
        assert_ne!(
            StructuredObjectHeader::compute_descriptor_hash(
                &fields(),
                StructuredObjectLayout::RowMajor
            ),
            StructuredObjectHeader::compute_descriptor_hash(&alt, StructuredObjectLayout::RowMajor)
        );
    }

    #[test]
    fn validate_rejects_descriptor_hash_drift() {
        let mut h = header("Reservation");
        h.descriptor_hash = ContractHash::test_vector(0xAA);
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("descriptor hash"));
    }

    #[test]
    fn validate_rejects_column_count_mismatch() {
        let mut h = header("Reservation");
        h.column_count = (h.fields.len() as u32) + 1;
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("column_count"));
    }

    #[test]
    fn validate_rejects_exact_required_without_exact_value() {
        let mut h = header("Reservation");
        h.row_count_policy = RowCountPolicy::ExactRequired;
        h.row_count_exact = None;
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("ExactRequired"));
    }

    #[test]
    fn validate_accepts_unknown_allowed_without_exact_value() {
        let mut h = header("Reservation");
        h.row_count_policy = RowCountPolicy::UnknownAllowed;
        h.row_count_exact = None;
        // payload-length coherence rule only fires when row_count_exact is
        // Some; with Unknown the header is otherwise valid.
        assert!(h.validate().is_ok());
    }

    #[test]
    fn validate_rejects_payload_when_zero_rows_declared() {
        let mut h = header("Reservation");
        h.row_count_exact = Some(0);
        h.payload_length = 1;
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
        assert!(err.message().contains("zero rows"));
    }

    #[test]
    fn validate_rejects_empty_payload_when_positive_rows_declared() {
        let mut h = header("Reservation");
        h.row_count_exact = Some(3);
        h.payload_length = 0;
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
        assert!(err.message().contains("empty payload"));
    }

    #[test]
    fn validate_rejects_payload_exceeding_max() {
        let mut h = header("Reservation");
        h.payload_length = h.max_payload_length.unwrap() + 1;
        let err = h.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
        assert!(err.message().contains("payload length"));
    }

    #[test]
    fn validate_accepts_well_formed_header() {
        assert!(header("Reservation").validate().is_ok());
    }
}
