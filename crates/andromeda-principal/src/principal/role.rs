use super::PermissionSet;

/// Principal roles in the RBAC model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalRole {
    SuperAdmin,
    Admin,
    Operator,
    User,
    Guest,
}

impl PrincipalRole {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "superadmin",
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::User => "user",
            Self::Guest => "guest",
        }
    }

    pub fn permissions(self) -> PermissionSet {
        PermissionSet::for_role(self)
    }

    fn parse_str(s: &str) -> Option<Self> {
        match s {
            "superadmin" => Some(Self::SuperAdmin),
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            "user" => Some(Self::User),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }
}

impl std::str::FromStr for PrincipalRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s).ok_or(())
    }
}

impl std::fmt::Display for PrincipalRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
