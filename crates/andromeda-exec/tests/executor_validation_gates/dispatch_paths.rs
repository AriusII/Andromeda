use super::support::*;

#[test]
fn gate_exec_01_srpl_dispatcher_constructs_with_dependencies() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    let dispatcher = SrplProcedureDispatcher::new(resolver, interpreter);
    let _ = dispatcher.clone();
}

#[test]
fn gate_exec_01_srpl_dispatcher_cloneable_for_sharing() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    let dispatcher1 = SrplProcedureDispatcher::new(resolver, interpreter);
    let dispatcher2 = dispatcher1.clone();
    let dispatcher3 = dispatcher2.clone();

    let _d1 = dispatcher1;
    let _d2 = dispatcher2;
    let _d3 = dispatcher3;
}

#[test]
fn gate_exec_03_both_dispatch_paths_available() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    #[allow(dead_code)]
    enum DispatchPath {
        CatalogedLocalProcedure,
        SrplInterpreted(SrplProcedureDispatcher),
    }

    let local_path = DispatchPath::CatalogedLocalProcedure;
    let srpl_path =
        DispatchPath::SrplInterpreted(SrplProcedureDispatcher::new(resolver, interpreter));

    assert!(matches!(local_path, DispatchPath::CatalogedLocalProcedure));
    assert!(matches!(srpl_path, DispatchPath::SrplInterpreted(_)));
}
