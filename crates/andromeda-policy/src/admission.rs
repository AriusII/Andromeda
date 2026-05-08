#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdmissionStance {
    Admit,
    Reject,
    AdvisoryOnly,
}

impl AdmissionStance {
    pub const VARIANT_COUNT: usize = 3;
    pub const ALL: [Self; Self::VARIANT_COUNT] = [Self::Admit, Self::Reject, Self::AdvisoryOnly];

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Admit => 0x01,
            Self::Reject => 0x02,
            Self::AdvisoryOnly => 0x03,
        }
    }

    pub const fn permits_dispatch(self) -> bool {
        matches!(self, Self::Admit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_stance_tags_are_stable_and_distinct() {
        let tags = AdmissionStance::ALL.map(AdmissionStance::as_tag);

        assert_eq!(tags, [0x01, 0x02, 0x03]);
        assert!(AdmissionStance::Admit.permits_dispatch());
        assert!(!AdmissionStance::Reject.permits_dispatch());
        assert!(!AdmissionStance::AdvisoryOnly.permits_dispatch());
    }
}
