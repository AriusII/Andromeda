#![no_main]

use andromeda_structured_object::{StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_CONTRACT_HASH_INPUT_BYTES: usize = 1024;
const MAX_DESCRIPTOR_FIELDS: usize = 8;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_CONTRACT_HASH_INPUT_BYTES);

    let parsed = ContractHash::from_slice(data);
    if data.len() == ContractHash::LEN {
        let Ok(hash) = parsed else {
            return;
        };
        let bytes = hash.as_bytes();
        assert_eq!(bytes.as_slice(), data);
        assert_eq!(ContractHash::from_slice(&bytes), Ok(hash));

        let rendered = hash.to_string();
        assert_eq!(rendered.len(), ContractHash::LEN * 2);
        assert!(rendered.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(hash.is_zero(), bytes.iter().all(|byte| *byte == 0));
    }

    let fields = descriptor_fields(data);
    let layout = layout_from(data.first().copied().unwrap_or_default());
    let first = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
    let second = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
    assert_eq!(first, second);
    assert_eq!(ContractHash::from_slice(&first.as_bytes()), Ok(first));
});

fn descriptor_fields(data: &[u8]) -> Vec<ColumnDescriptor> {
    let field_count = data
        .get(ContractHash::LEN)
        .map(|value| usize::from(*value) % (MAX_DESCRIPTOR_FIELDS + 1))
        .unwrap_or_default();

    (0..field_count)
        .map(|index| {
            let selector = data
                .get(ContractHash::LEN + 1 + index)
                .copied()
                .unwrap_or_default();
            ColumnDescriptor {
                name: format!("f{index}"),
                data_type: TypeDescriptor::required(scalar_from(selector)),
                ordinal: index as u32,
            }
        })
        .collect()
}

fn scalar_from(selector: u8) -> ScalarType {
    match selector % 6 {
        0 => ScalarType::I64,
        1 => ScalarType::U64,
        2 => ScalarType::Bool,
        3 => ScalarType::I32,
        4 => ScalarType::U32,
        _ => ScalarType::I16,
    }
}

fn layout_from(selector: u8) -> StructuredObjectLayout {
    match selector % 3 {
        0 => StructuredObjectLayout::RowMajor,
        1 => StructuredObjectLayout::ColumnMajor,
        _ => StructuredObjectLayout::Hybrid,
    }
}
