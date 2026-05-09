#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Security Contract

Runtime-free public security contracts shared by IAM, RPC metadata, manifests,
audit evidence, and operator tooling.

This crate owns stable security surface names, permission families, canonical
permission identifiers, policy evidence shapes, and admission codes only. It does not own
certificate parsing, principal registries, authorization evaluation, audit
sinks, QUIC/TLS runtime behavior, WAL, storage, or recovery logic.

The types in this crate are public contract types, not persistent or network
wire formats. Persistent and network boundaries must continue to use explicit
codecs or generated protocol contracts.
"#]

mod admission;
mod error;
mod permission;
mod policy;
pub mod principal;
mod surface;

pub use admission::{
    ALL_SECURITY_ADMISSION_V0_BOUNDARIES, ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES,
    ALL_SECURITY_ADMISSION_V0_OUTCOMES, ALL_SECURITY_ADMISSION_V0_REASON_CODES,
    ALL_SECURITY_ADMISSION_V0_STEPS, ALL_SURFACE_CLASSES, AdmissionDecision, PermissionRequest,
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION,
    SECURITY_ADMISSION_V0_CONTRACT_ID, SECURITY_ADMISSION_V0_SCHEMA_VERSION,
    SURFACE_CLASS_ID_ADMINISTRATION, SURFACE_CLASS_ID_APPLICATION, SURFACE_CLASS_ID_FORENSIC,
    SURFACE_CLASS_ID_HADR, SURFACE_CLASS_ID_MONITORING, SURFACE_CLASS_ID_RECOVERY,
    SecurityAdmissionAuditEventV0, SecurityAdmissionBoundaryV0, SecurityAdmissionEvidenceCodeV0,
    SecurityAdmissionOutcomeV0, SecurityAdmissionReasonCodeV0, SecurityAdmissionStepV0,
    SecurityAdmissionV0, SurfaceClass,
};
pub use error::SecurityContractError;
pub use permission::{
    ALL_PERMISSION_FAMILIES, ALL_PERMISSIONS, FAMILY_ID_APPLICATION, FAMILY_ID_CLUSTER,
    FAMILY_ID_DEFINITION, FAMILY_ID_DIAGNOSTICS, FAMILY_ID_RECOVERY, FAMILY_ID_SECURITY,
    PERMISSION_ID_BACKUP, PERMISSION_ID_CLUSTER_FENCE_NODE, PERMISSION_ID_CLUSTER_PROMOTE,
    PERMISSION_ID_CLUSTER_UPDATE_MANIFEST, PERMISSION_ID_CREATE_MAP,
    PERMISSION_ID_CREATE_PROCEDURE, PERMISSION_ID_CREATE_TABLE, PERMISSION_ID_DEBUG_PROCEDURE,
    PERMISSION_ID_EXECUTE_PROCEDURE, PERMISSION_ID_FORENSIC_START,
    PERMISSION_ID_IMPORT_DEFINITION_BATCH, PERMISSION_ID_INSPECT_PLANS,
    PERMISSION_ID_MANAGE_SECURITY, PERMISSION_ID_READ_AUDIT, PERMISSION_ID_READ_CONTRACT,
    PERMISSION_ID_READ_CONTRACT_METADATA, PERMISSION_ID_READ_PROCEDURE_STORE,
    PERMISSION_ID_RESTORE, PERMISSION_ID_REVOKE_CERTIFICATE_IDENTITY,
    PERMISSION_ID_ROTATE_CERTIFICATE, Permission, PermissionDescriptor, PermissionFamily,
    canonical_permission_id, permission_family_for_id, permission_from_canonical_id,
};
pub use policy::{
    SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, SECURITY_POLICY_VERSION_LEN, SecurityPolicyEvidence,
    SecurityPolicyVersion,
};
pub use principal::{Permission as PrincipalPermission, SurfaceScope};
pub use surface::{
    ALL_SECURITY_SURFACES, SURFACE_ID_ADMINISTRATION, SURFACE_ID_APPLICATION,
    SURFACE_ID_BACKUP_AGENT, SURFACE_ID_CLUSTER, SURFACE_ID_MONITORING_AGENT, SecuritySurface,
    SecuritySurfacePlane, SurfacePlaneBinding, security_surface_from_id,
    surface_permits_permission,
};

pub type SecuritySurfaceScope = SecuritySurface;
pub type SecurityPermission = Permission;
pub type SecurityPermissionFamily = PermissionFamily;
