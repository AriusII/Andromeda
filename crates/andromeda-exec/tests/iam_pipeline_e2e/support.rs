use andromeda_core::PrincipalRole;
use andromeda_exec::services::{
    ConcretePermissionEvaluator, LocalPrincipalResolver, PrincipalResolver,
};
use std::sync::Arc;

pub(crate) fn new_resolver() -> Arc<LocalPrincipalResolver> {
    Arc::new(LocalPrincipalResolver::new())
}

pub(crate) fn resolver_with_principal(
    fingerprint: &str,
    role: PrincipalRole,
) -> Arc<LocalPrincipalResolver> {
    let resolver = new_resolver();
    resolver
        .register_principal(fingerprint.to_string(), role)
        .expect("registration should succeed");
    resolver
}

pub(crate) fn evaluator_for(
    resolver: Arc<LocalPrincipalResolver>,
) -> ConcretePermissionEvaluator<LocalPrincipalResolver> {
    ConcretePermissionEvaluator::new(resolver)
}

pub(crate) fn resolver_and_evaluator(
    fingerprint: &str,
    role: PrincipalRole,
) -> (
    Arc<LocalPrincipalResolver>,
    ConcretePermissionEvaluator<LocalPrincipalResolver>,
) {
    let resolver = resolver_with_principal(fingerprint, role);
    let evaluator = evaluator_for(Arc::clone(&resolver));
    (resolver, evaluator)
}
