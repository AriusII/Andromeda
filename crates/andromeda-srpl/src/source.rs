use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::ForbiddenConstruct;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplSource<'a> {
    pub text: &'a str,
}

impl<'a> SrplSource<'a> {
    pub const fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn forbidden_constructs(&self) -> Vec<ForbiddenConstruct> {
        let normalized = self.text.to_ascii_lowercase();
        let mut constructs = Vec::new();

        if normalized.contains("while ") {
            constructs.push(ForbiddenConstruct::UnboundedWhile);
        }
        if normalized.contains("recursive") || normalized.contains("call self") {
            constructs.push(ForbiddenConstruct::FreeRecursion);
        }
        if normalized.contains("http://") || normalized.contains("https://") {
            constructs.push(ForbiddenConstruct::ExternalNetwork);
        }
        if normalized.contains("filesystem") || normalized.contains("file://") {
            constructs.push(ForbiddenConstruct::ExternalFilesystem);
        }
        if normalized.contains("random(") || normalized.contains("rand(") {
            constructs.push(ForbiddenConstruct::NondeterministicRandom);
        }
        if normalized.contains("dynamic sql") || normalized.contains("execute sql") {
            constructs.push(ForbiddenConstruct::DynamicTextSql);
        }
        if normalized.contains("select *") {
            constructs.push(ForbiddenConstruct::SelectStar);
        }

        constructs
    }

    pub fn validate_for_core_language(&self) -> AndromedaResult<()> {
        let constructs = self.forbidden_constructs();
        if constructs.is_empty() {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("forbidden SRPL core construct: {:?}", constructs[0]),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_core_rejects_unbounded_and_dynamic_constructs() {
        let source =
            SrplSource::new("procedure X accepts A i64 begin while true do execute sql end;");
        let constructs = source.forbidden_constructs();

        assert!(constructs.contains(&ForbiddenConstruct::UnboundedWhile));
        assert!(constructs.contains(&ForbiddenConstruct::DynamicTextSql));
        assert_eq!(
            source.validate_for_core_language().unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
    }
}
