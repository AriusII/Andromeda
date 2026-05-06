/// Determinism classification for SRPL built-in functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionDeterminism {
    /// Pure function: same inputs → same output always. FOLDABLE when
    /// all arguments are compile-time constants.
    PureDeterministic,
    /// Value varies per transaction (e.g. `TRANSACTION_TIMESTAMP`).
    /// Must **never** be folded.
    NonDeterministicPerTransaction,
    /// Value varies per invocation (e.g. `NOW`, `RAND`, `UUID`).
    /// Must **never** be folded.
    NonDeterministicPerInvocation,
}

impl FunctionDeterminism {
    /// True only for `PureDeterministic`. All other variants return false.
    pub const fn is_foldable(self) -> bool {
        matches!(self, Self::PureDeterministic)
    }
}

/// Classify a built-in function by name.
///
/// The registry is a closed `match`. Any name not listed returns
/// `NonDeterministicPerInvocation` — the safe pessimistic default.
///
/// Adding a new pure-deterministic name to the registry is a backwards-
/// compatible change but requires a doctrine review to confirm the
/// function is truly free of side-effects and external state.
pub fn classify_builtin(name: &str) -> FunctionDeterminism {
    match name {
        "DATE_ADD" | "DATE_SUB" | "ABS" | "COALESCE" | "LENGTH" | "UPPER" | "LOWER" | "TRIM"
        | "LTRIM" | "RTRIM" | "CHAR_LENGTH" | "CONCAT" | "REPLACE" | "SUBSTRING" | "MOD"
        | "POWER" | "FLOOR" | "CEILING" | "ROUND" | "SIGN" | "GREATEST" | "LEAST" => {
            FunctionDeterminism::PureDeterministic
        }

        "TRANSACTION_TIMESTAMP" | "LOCALTIMESTAMP" | "CURRENT_DATE" | "CURRENT_TIME" => {
            FunctionDeterminism::NonDeterministicPerTransaction
        }

        "NOW" | "RAND" | "RANDOM" | "UUID" | "NEWID" | "GEN_RANDOM_UUID" | "CURRENT_TIMESTAMP"
        | "SYSDATE" | "GETDATE" | "GETUTCDATE" | "SYSDATETIME" => {
            FunctionDeterminism::NonDeterministicPerInvocation
        }
        _ => FunctionDeterminism::NonDeterministicPerInvocation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_nondeterministic_per_invocation() {
        assert_eq!(
            classify_builtin("NOW"),
            FunctionDeterminism::NonDeterministicPerInvocation
        );
    }

    #[test]
    fn rand_is_nondeterministic_per_invocation() {
        assert_eq!(
            classify_builtin("RAND"),
            FunctionDeterminism::NonDeterministicPerInvocation
        );
    }

    #[test]
    fn uuid_is_nondeterministic_per_invocation() {
        assert_eq!(
            classify_builtin("UUID"),
            FunctionDeterminism::NonDeterministicPerInvocation
        );
    }

    #[test]
    fn newid_is_nondeterministic_per_invocation() {
        assert_eq!(
            classify_builtin("NEWID"),
            FunctionDeterminism::NonDeterministicPerInvocation
        );
    }

    #[test]
    fn transaction_timestamp_is_nondeterministic_per_transaction() {
        assert_eq!(
            classify_builtin("TRANSACTION_TIMESTAMP"),
            FunctionDeterminism::NonDeterministicPerTransaction
        );
    }

    #[test]
    fn date_add_is_pure_deterministic() {
        assert_eq!(
            classify_builtin("DATE_ADD"),
            FunctionDeterminism::PureDeterministic
        );
    }

    #[test]
    fn abs_is_pure_deterministic() {
        assert_eq!(
            classify_builtin("ABS"),
            FunctionDeterminism::PureDeterministic
        );
    }

    #[test]
    fn unknown_function_defaults_to_nondeterministic_per_invocation() {
        assert_eq!(
            classify_builtin("SOME_UNKNOWN_FUNCTION_XYZ"),
            FunctionDeterminism::NonDeterministicPerInvocation
        );
    }

    #[test]
    fn now_is_not_foldable() {
        assert!(!classify_builtin("NOW").is_foldable());
    }

    #[test]
    fn abs_is_foldable() {
        assert!(classify_builtin("ABS").is_foldable());
    }
}
