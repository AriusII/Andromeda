use andromeda_audit::{
    AdminOperation, DurableAuditEventFamily, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditTraceFamily, Permission, SurfaceScope,
};
use andromeda_observability::TraceEventFamily;

pub(super) fn trace_family_str(family: TraceEventFamily) -> &'static str {
    match family {
        TraceEventFamily::Decision => "decision",
        TraceEventFamily::ProcedureInvocation => "procedure-invocation",
        TraceEventFamily::Wal => "wal",
        TraceEventFamily::Recovery => "recovery",
        TraceEventFamily::ManifestCatalog => "manifest-catalog",
        TraceEventFamily::Protocol => "protocol",
        TraceEventFamily::SecurityAudit => "security-audit",
        TraceEventFamily::AdminAudit => "admin-audit",
        TraceEventFamily::Resource => "resource",
        TraceEventFamily::Io => "io",
        TraceEventFamily::Gpu => "gpu",
        TraceEventFamily::Transaction => "transaction",
    }
}

pub(super) fn durable_family_str(family: DurableAuditEventFamily) -> &'static str {
    match family {
        DurableAuditEventFamily::SecurityDecision => "security-decision",
        DurableAuditEventFamily::AdminDecision => "admin-decision",
        DurableAuditEventFamily::AdmissionDecision => "admission-decision",
        DurableAuditEventFamily::CatalogDecision => "catalog-decision",
        DurableAuditEventFamily::HadrDecision => "hadr-decision",
        DurableAuditEventFamily::BackupDecision => "backup-decision",
        DurableAuditEventFamily::RestoreDecision => "restore-decision",
        DurableAuditEventFamily::ForensicDecision => "forensic-decision",
        DurableAuditEventFamily::RecoveryDecision => "recovery-decision",
        DurableAuditEventFamily::GenericAudit => "generic-audit",
    }
}

pub(super) fn durable_trace_family_str(family: DurableAuditTraceFamily) -> &'static str {
    match family {
        DurableAuditTraceFamily::Decision => "decision",
        DurableAuditTraceFamily::ProcedureInvocation => "procedure-invocation",
        DurableAuditTraceFamily::Wal => "wal",
        DurableAuditTraceFamily::Recovery => "recovery",
        DurableAuditTraceFamily::ManifestCatalog => "manifest-catalog",
        DurableAuditTraceFamily::Protocol => "protocol",
        DurableAuditTraceFamily::SecurityAudit => "security-audit",
        DurableAuditTraceFamily::AdminAudit => "admin-audit",
        DurableAuditTraceFamily::Resource => "resource",
        DurableAuditTraceFamily::Io => "io",
        DurableAuditTraceFamily::Gpu => "gpu",
        DurableAuditTraceFamily::Transaction => "transaction",
    }
}

pub(super) fn replay_behavior_str(value: DurableAuditReplayBehavior) -> &'static str {
    match value {
        DurableAuditReplayBehavior::ForensicOnly => "forensic-only",
        DurableAuditReplayBehavior::RebuildDecisionIndex => "rebuild-decision-index",
        DurableAuditReplayBehavior::CorruptionBoundary => "corruption-boundary",
    }
}

pub(super) fn retention_str(value: DurableAuditRetentionBoundary) -> &'static str {
    match value {
        DurableAuditRetentionBoundary::WalSegment => "wal-segment",
        DurableAuditRetentionBoundary::CatalogVersion => "catalog-version",
        DurableAuditRetentionBoundary::SecurityPolicy => "security-policy",
        DurableAuditRetentionBoundary::ForensicHold => "forensic-hold",
    }
}

pub(super) fn surface_str(surface: SurfaceScope) -> &'static str {
    match surface {
        SurfaceScope::Application => "application",
        SurfaceScope::Administration => "administration",
        SurfaceScope::Cluster => "cluster",
        SurfaceScope::BackupAgent => "backup-agent",
        SurfaceScope::MonitoringAgent => "monitoring-agent",
    }
}

pub(super) fn permission_str(permission: Permission) -> &'static str {
    match permission {
        Permission::ExecuteProcedure => "execute-procedure",
        Permission::ReadContract => "read-contract",
        Permission::CreateTable => "create-table",
        Permission::CreateMap => "create-map",
        Permission::CreateProcedure => "create-procedure",
        Permission::ImportDefinitionBatch => "import-definition-batch",
        Permission::DebugProcedure => "debug-procedure",
        Permission::ReadProcedureStore => "read-procedure-store",
        Permission::InspectPlans => "inspect-plans",
        Permission::ManageSecurity => "manage-security",
        Permission::RotateCertificate => "rotate-certificate",
        Permission::RevokeCertificateIdentity => "revoke-certificate-identity",
        Permission::Backup => "backup",
        Permission::Restore => "restore",
        Permission::ForensicStart => "forensic-start",
        Permission::ClusterPromote => "cluster-promote",
        Permission::FenceNode => "fence-node",
        Permission::UpdateClusterManifest => "update-cluster-manifest",
    }
}

pub(super) fn admin_operation_str(operation: AdminOperation) -> &'static str {
    match operation {
        AdminOperation::DebugProcedure => "debug-procedure",
        AdminOperation::ReadProcedureStore => "read-procedure-store",
        AdminOperation::InspectPlans => "inspect-plans",
        AdminOperation::ManageSecurity => "manage-security",
        AdminOperation::RotateCertificate => "rotate-certificate",
        AdminOperation::RevokeCertificateIdentity => "revoke-certificate-identity",
        AdminOperation::Backup => "backup",
        AdminOperation::Restore => "restore",
        AdminOperation::ForensicStart => "forensic-start",
        AdminOperation::ClusterPromote => "cluster-promote",
        AdminOperation::FenceNode => "fence-node",
        AdminOperation::UpdateClusterManifest => "update-cluster-manifest",
    }
}
