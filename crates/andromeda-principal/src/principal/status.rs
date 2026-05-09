/// Lifecycle status for a user principal.
///
/// Disabled principals remain registered for audit and recovery evidence, but
/// they must not authorize any permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalStatus {
    Active,
    Disabled,
}

impl PrincipalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl std::fmt::Display for PrincipalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
