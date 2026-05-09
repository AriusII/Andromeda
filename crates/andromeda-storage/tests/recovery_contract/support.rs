use andromeda_storage::format_version::{FormatVersion, StorageFormatKind};
use andromeda_storage::{RECOVERY_REQUIRED_STORAGE_FORMATS, StorageFormatFingerprint};

pub(crate) fn recovery_v1_format_fingerprints_with(
    override_kind: StorageFormatKind,
    override_version: FormatVersion,
) -> Vec<StorageFormatFingerprint> {
    RECOVERY_REQUIRED_STORAGE_FORMATS
        .iter()
        .map(|kind| {
            StorageFormatFingerprint::new(
                *kind,
                if *kind == override_kind {
                    override_version
                } else {
                    FormatVersion::V1_0
                },
            )
        })
        .collect()
}
