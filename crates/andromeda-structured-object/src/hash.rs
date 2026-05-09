use andromeda_digest::Sha256;
use andromeda_types::{
    ColumnDescriptor, ContractHash, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding,
    TimestampType,
};

use super::StructuredObjectLayout;

pub(super) fn compute_descriptor_hash(
    fields: &[ColumnDescriptor],
    layout: StructuredObjectLayout,
) -> ContractHash {
    let mut sink = StructuredObjectHashSink::new();
    sink.tag("andromeda.structured-object.descriptor.v1.sha256");
    sink.layout(layout);
    sink.columns(fields);
    sink.finish()
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
        self.u8(layout.hash_tag());
    }

    fn columns(&mut self, columns: &[ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.bytes(column.name.as_bytes());
            self.u32(column.ordinal);
            self.scalar(&column.data_type.scalar);
            self.u8(match column.data_type.absence {
                andromeda_types::AbsencePolicy::Required => 0,
                andromeda_types::AbsencePolicy::ExplicitOptional => 1,
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
                    },
                }
            },
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
                    },
                }
            },
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
                    },
                    None => self.bool(false),
                }
            },
            ScalarType::Timestamp(timestamp) => {
                self.u8(14);
                self.u8(match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                });
            },
        }
    }
}
