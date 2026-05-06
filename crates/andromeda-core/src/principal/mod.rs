//! Principal identity, roles, and permissions for the IAM pipeline.

mod certificate;
mod id;
mod permission;
mod permission_set;
mod principal;
mod role;
mod session;

pub use certificate::CertificateFingerprint;
pub use id::PrincipalId;
pub use permission::Permission;
pub use permission_set::PermissionSet;
pub use principal::Principal;
pub use role::PrincipalRole;
pub use session::SessionToken;

#[cfg(test)]
mod tests;
