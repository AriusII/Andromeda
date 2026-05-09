use super::super::{
    DurableAuditEventFamily, DurableAuditReplayBehavior, DurableAuditRetentionBoundary,
};
use super::NONE_FIELD;
use crate::events::{Permission, SecurityPolicyVersionEvidence, SurfaceScope};

pub(super) fn parse_hex_checksum(field: &str, label: &str) -> Result<u64, String> {
    if field.len() != 16 || !field.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} field must be 16 ASCII hex characters"));
    }
    u64::from_str_radix(field, 16).map_err(|_| format!("{label} field is not valid hex"))
}

pub(super) fn parse_usize_field(field: &str, label: &str) -> Result<usize, String> {
    strip_field(field, label)?
        .parse::<usize>()
        .map_err(|_| format!("{label} field is not a valid usize"))
}

pub(super) fn parse_u64_field(field: &str, label: &str) -> Result<u64, String> {
    strip_field(field, label)?
        .parse::<u64>()
        .map_err(|_| format!("{label} field is not a valid u64"))
}

pub(super) fn parse_u128_field(field: &str, label: &str) -> Result<u128, String> {
    strip_field(field, label)?
        .parse::<u128>()
        .map_err(|_| format!("{label} field is not a valid u128"))
}

pub(super) fn parse_optional_u64(value: &str) -> Result<Option<u64>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| "optional integer field is not valid u64".to_string())
}

pub(super) fn parse_optional_policy_version(
    policy_version_value: &str,
    policy_digest_value: &str,
) -> Result<Option<SecurityPolicyVersionEvidence>, String> {
    let policy_version = parse_optional_u64(policy_version_value)?;
    let policy_digest = decode_optional_string(policy_digest_value)?;
    match (policy_version, policy_digest) {
        (None, None) => Ok(None),
        (Some(policy_version), Some(policy_digest)) => {
            SecurityPolicyVersionEvidence::new(policy_version, policy_digest)
                .map(Some)
                .map_err(|error| error.message().to_string())
        },
        _ => Err(
            "durable audit policy version evidence requires both policy_version and policy_digest"
                .to_string(),
        ),
    }
}

pub(super) fn strip_field<'a>(field: &'a str, label: &str) -> Result<&'a str, String> {
    field
        .strip_prefix(label)
        .and_then(|rest| rest.strip_prefix('='))
        .ok_or_else(|| format!("missing {label} field"))
}

pub(super) fn encode_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

pub(super) fn encode_optional_string(value: Option<&str>) -> String {
    value
        .map(encode_string)
        .unwrap_or_else(|| NONE_FIELD.to_string())
}

pub(super) fn decode_string(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = decode_hex_nibble(pair[0])?;
        let low = decode_hex_nibble(pair[1])?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).map_err(|_| "hex string is not valid UTF-8".to_string())
}

fn decode_hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("hex string contains non-hex bytes".to_string()),
    }
}

pub(super) fn decode_optional_string(value: &str) -> Result<Option<String>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    decode_string(value).map(Some)
}

pub(super) fn format_family(value: DurableAuditEventFamily) -> &'static str {
    match value {
        DurableAuditEventFamily::SecurityDecision => "SecurityDecision",
        DurableAuditEventFamily::AdminDecision => "AdminDecision",
        DurableAuditEventFamily::AdmissionDecision => "AdmissionDecision",
        DurableAuditEventFamily::CatalogDecision => "CatalogDecision",
        DurableAuditEventFamily::HadrDecision => "HadrDecision",
        DurableAuditEventFamily::BackupDecision => "BackupDecision",
        DurableAuditEventFamily::RestoreDecision => "RestoreDecision",
        DurableAuditEventFamily::ForensicDecision => "ForensicDecision",
        DurableAuditEventFamily::RecoveryDecision => "RecoveryDecision",
        DurableAuditEventFamily::GenericAudit => "GenericAudit",
    }
}

pub(super) fn parse_family(value: &str) -> Result<DurableAuditEventFamily, String> {
    match value {
        "SecurityDecision" => Ok(DurableAuditEventFamily::SecurityDecision),
        "AdminDecision" => Ok(DurableAuditEventFamily::AdminDecision),
        "AdmissionDecision" => Ok(DurableAuditEventFamily::AdmissionDecision),
        "CatalogDecision" => Ok(DurableAuditEventFamily::CatalogDecision),
        "HadrDecision" => Ok(DurableAuditEventFamily::HadrDecision),
        "BackupDecision" => Ok(DurableAuditEventFamily::BackupDecision),
        "RestoreDecision" => Ok(DurableAuditEventFamily::RestoreDecision),
        "ForensicDecision" => Ok(DurableAuditEventFamily::ForensicDecision),
        "RecoveryDecision" => Ok(DurableAuditEventFamily::RecoveryDecision),
        "GenericAudit" => Ok(DurableAuditEventFamily::GenericAudit),
        _ => Err("unknown durable audit family".to_string()),
    }
}

pub(super) fn format_replay(value: DurableAuditReplayBehavior) -> &'static str {
    match value {
        DurableAuditReplayBehavior::ForensicOnly => "ForensicOnly",
        DurableAuditReplayBehavior::RebuildDecisionIndex => "RebuildDecisionIndex",
        DurableAuditReplayBehavior::CorruptionBoundary => "CorruptionBoundary",
    }
}

pub(super) fn parse_replay(value: &str) -> Result<DurableAuditReplayBehavior, String> {
    match value {
        "ForensicOnly" => Ok(DurableAuditReplayBehavior::ForensicOnly),
        "RebuildDecisionIndex" => Ok(DurableAuditReplayBehavior::RebuildDecisionIndex),
        "CorruptionBoundary" => Ok(DurableAuditReplayBehavior::CorruptionBoundary),
        _ => Err("unknown durable audit replay behavior".to_string()),
    }
}

pub(super) fn format_retention(value: DurableAuditRetentionBoundary) -> &'static str {
    match value {
        DurableAuditRetentionBoundary::WalSegment => "WalSegment",
        DurableAuditRetentionBoundary::CatalogVersion => "CatalogVersion",
        DurableAuditRetentionBoundary::SecurityPolicy => "SecurityPolicy",
        DurableAuditRetentionBoundary::ForensicHold => "ForensicHold",
    }
}

pub(super) fn parse_retention(value: &str) -> Result<DurableAuditRetentionBoundary, String> {
    match value {
        "WalSegment" => Ok(DurableAuditRetentionBoundary::WalSegment),
        "CatalogVersion" => Ok(DurableAuditRetentionBoundary::CatalogVersion),
        "SecurityPolicy" => Ok(DurableAuditRetentionBoundary::SecurityPolicy),
        "ForensicHold" => Ok(DurableAuditRetentionBoundary::ForensicHold),
        _ => Err("unknown durable audit retention boundary".to_string()),
    }
}

pub(super) fn format_surface(value: SurfaceScope) -> &'static str {
    match value {
        SurfaceScope::Application => "Application",
        SurfaceScope::Administration => "Administration",
        SurfaceScope::Cluster => "Cluster",
        SurfaceScope::BackupAgent => "BackupAgent",
        SurfaceScope::MonitoringAgent => "MonitoringAgent",
    }
}

pub(super) fn parse_optional_surface(value: &str) -> Result<Option<SurfaceScope>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "Application" => Ok(Some(SurfaceScope::Application)),
        "Administration" => Ok(Some(SurfaceScope::Administration)),
        "Cluster" => Ok(Some(SurfaceScope::Cluster)),
        "BackupAgent" => Ok(Some(SurfaceScope::BackupAgent)),
        "MonitoringAgent" => Ok(Some(SurfaceScope::MonitoringAgent)),
        _ => Err("unknown durable audit surface scope".to_string()),
    }
}

pub(super) fn format_permission(value: Permission) -> &'static str {
    match value {
        Permission::ExecuteProcedure => "ExecuteProcedure",
        Permission::ReadContract => "ReadContract",
        Permission::CreateTable => "CreateTable",
        Permission::CreateMap => "CreateMap",
        Permission::CreateProcedure => "CreateProcedure",
        Permission::ImportDefinitionBatch => "ImportDefinitionBatch",
        Permission::DebugProcedure => "DebugProcedure",
        Permission::ReadProcedureStore => "ReadProcedureStore",
        Permission::InspectPlans => "InspectPlans",
        Permission::ManageSecurity => "ManageSecurity",
        Permission::RotateCertificate => "RotateCertificate",
        Permission::RevokeCertificateIdentity => "RevokeCertificateIdentity",
        Permission::Backup => "Backup",
        Permission::Restore => "Restore",
        Permission::ForensicStart => "ForensicStart",
        Permission::ClusterPromote => "ClusterPromote",
        Permission::FenceNode => "FenceNode",
        Permission::UpdateClusterManifest => "UpdateClusterManifest",
    }
}

pub(super) fn parse_optional_permission(value: &str) -> Result<Option<Permission>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "ExecuteProcedure" => Ok(Some(Permission::ExecuteProcedure)),
        "ReadContract" => Ok(Some(Permission::ReadContract)),
        "CreateTable" => Ok(Some(Permission::CreateTable)),
        "CreateMap" => Ok(Some(Permission::CreateMap)),
        "CreateProcedure" => Ok(Some(Permission::CreateProcedure)),
        "ImportDefinitionBatch" => Ok(Some(Permission::ImportDefinitionBatch)),
        "DebugProcedure" => Ok(Some(Permission::DebugProcedure)),
        "ReadProcedureStore" => Ok(Some(Permission::ReadProcedureStore)),
        "InspectPlans" => Ok(Some(Permission::InspectPlans)),
        "ManageSecurity" => Ok(Some(Permission::ManageSecurity)),
        "RotateCertificate" => Ok(Some(Permission::RotateCertificate)),
        "RevokeCertificateIdentity" => Ok(Some(Permission::RevokeCertificateIdentity)),
        "Backup" => Ok(Some(Permission::Backup)),
        "Restore" => Ok(Some(Permission::Restore)),
        "ForensicStart" => Ok(Some(Permission::ForensicStart)),
        "ClusterPromote" => Ok(Some(Permission::ClusterPromote)),
        "FenceNode" => Ok(Some(Permission::FenceNode)),
        "UpdateClusterManifest" => Ok(Some(Permission::UpdateClusterManifest)),
        _ => Err("unknown durable audit permission".to_string()),
    }
}
