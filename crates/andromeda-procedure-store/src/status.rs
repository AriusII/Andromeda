#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvocationStatus {
    Admitted,
    Rejected,
    Started,
    Committed,
    RolledBack,
    Failed,
    Aborted,
}

impl InvocationStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Rejected | Self::Committed | Self::RolledBack | Self::Failed | Self::Aborted
        )
    }

    pub const fn completed_successfully(self) -> bool {
        matches!(self, Self::Committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_status_marks_terminal_states() {
        assert!(!InvocationStatus::Admitted.is_terminal());
        assert!(!InvocationStatus::Started.is_terminal());
        assert!(InvocationStatus::Rejected.is_terminal());
        assert!(InvocationStatus::Committed.is_terminal());
        assert!(InvocationStatus::RolledBack.is_terminal());
        assert!(InvocationStatus::Failed.is_terminal());
        assert!(InvocationStatus::Aborted.is_terminal());
        assert!(InvocationStatus::Committed.completed_successfully());
        assert!(!InvocationStatus::Failed.completed_successfully());
    }
}
