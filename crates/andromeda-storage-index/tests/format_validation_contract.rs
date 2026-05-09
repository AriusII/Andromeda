use andromeda_storage_index::{BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatGate};

#[test]
fn key_v1_format_gate_rejects_mutations_before_promotion() {
    let gate = KeyV1FormatGate::new(1, 0, BTreeKeyFormatIdentity::V1_0);
    let error = gate
        .validate_operation(BTreeOperationType::Insert)
        .expect_err("mutations must remain gated");
    assert!(error.message().contains("DEC-038"));
}

#[test]
fn key_v1_format_gate_rejects_storage_and_key_major_mismatch() {
    let gate = KeyV1FormatGate::new(2, 0, BTreeKeyFormatIdentity::V1_0);
    let error = gate
        .validate_operation(BTreeOperationType::Lookup)
        .expect_err("major mismatch must be rejected");
    assert!(error.message().contains("incompatible"));
}
