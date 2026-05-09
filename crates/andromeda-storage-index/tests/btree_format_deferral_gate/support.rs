use andromeda_storage_index::{BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator};

pub(crate) fn key_v1_validator() -> KeyV1FormatValidator {
    KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::V1_0)
}

pub(crate) fn unknown_major_validator() -> KeyV1FormatValidator {
    KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::new(99, 0, 1, 4096))
}

pub(crate) fn unknown_codec_validator() -> KeyV1FormatValidator {
    KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::new(1, 0, 99, 4096))
}

pub(crate) fn mutation_error_message(operation: BTreeOperationType) -> String {
    key_v1_validator()
        .validate_operation(operation)
        .expect_err("durable B-Tree mutation must be gated before promotion")
        .message()
        .to_string()
}
