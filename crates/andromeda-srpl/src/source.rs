use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{ForbiddenConstruct, ForbiddenConstructHit, SourceSpan, SrplDiagnostic};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplSource<'a> {
    pub text: &'a str,
}

impl<'a> SrplSource<'a> {
    pub const fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn forbidden_constructs(&self) -> Vec<ForbiddenConstruct> {
        self.forbidden_construct_hits()
            .into_iter()
            .map(|hit| hit.construct)
            .collect()
    }

    pub fn forbidden_construct_hits(&self) -> Vec<ForbiddenConstructHit> {
        let normalized = self.text.to_ascii_lowercase();
        let mut constructs = Vec::new();

        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::UnboundedWhile,
            &["while "],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::FreeRecursion,
            &["recursive", "call self"],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::ExternalNetwork,
            &["http://", "https://"],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::ExternalFilesystem,
            &["filesystem", "file://"],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::NondeterministicRandom,
            &["random(", "rand("],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::DynamicTextSql,
            &["dynamic sql", "execute sql"],
        );
        push_first_match(
            &normalized,
            &mut constructs,
            ForbiddenConstruct::SelectStar,
            &["select *"],
        );

        constructs
    }

    pub fn forbidden_construct_diagnostics(&self) -> Vec<SrplDiagnostic> {
        self.forbidden_construct_hits()
            .into_iter()
            .map(ForbiddenConstructHit::diagnostic)
            .collect()
    }

    pub fn validate_for_core_language(&self) -> AndromedaResult<()> {
        let diagnostics = self.forbidden_construct_diagnostics();
        if diagnostics.is_empty() {
            return Ok(());
        }

        let first = &diagnostics[0];
        let location = first
            .location
            .map(|span| format!(" at byte {}..{}", span.start, span.end))
            .unwrap_or_default();

        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{}{}", first.message, location),
        ))
    }
}

fn push_first_match(
    normalized: &str,
    constructs: &mut Vec<ForbiddenConstructHit>,
    construct: ForbiddenConstruct,
    patterns: &[&str],
) {
    let Some((start, pattern)) = patterns
        .iter()
        .filter_map(|pattern| normalized.find(pattern).map(|start| (start, *pattern)))
        .min_by_key(|(start, _)| *start)
    else {
        return;
    };

    constructs.push(ForbiddenConstructHit::new(
        construct,
        SourceSpan::new(start, start + pattern.len()),
    ));
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

    #[test]
    fn srpl_forbidden_construct_diagnostics_include_conceptual_span() {
        let source = SrplSource::new("procedure X begin select * from Inventory.Product end;");
        let diagnostics = source.forbidden_construct_diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].phase, crate::DiagnosticPhase::Binding);
        assert_eq!(diagnostics[0].location, Some(SourceSpan::new(18, 26)));
        assert!(diagnostics[0].message.contains("select star"));
    }
}
