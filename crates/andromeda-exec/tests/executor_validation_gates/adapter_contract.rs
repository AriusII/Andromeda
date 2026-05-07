use super::support::*;

#[test]
fn gate_exec_04_adapter_implements_dispatcher_trait() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);

    let _trait_obj: &dyn ProcedureDispatcher = &adapter;
}

#[test]
fn gate_exec_04_adapter_cloneable() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter1 = SrplDispatcherAdapter::new(dispatcher);

    let adapter2 = adapter1.clone();
    let _adapter3 = adapter2.clone();
}

#[test]
fn gate_exec_05_dispatcher_thread_safe() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = Arc::new(srpl_dispatcher(resolver));

    let mut handles = vec![];
    for i in 0..10 {
        let dispatcher_clone = Arc::clone(&dispatcher);
        let handle = std::thread::spawn(move || {
            let _dispatcher = dispatcher_clone;
            i
        });
        handles.push(handle);
    }

    let moved_count = handles.len();
    for handle in handles {
        handle.join().expect("dispatcher thread must not panic");
    }
    assert_eq!(moved_count, 10);
}

#[test]
fn gate_exec_05_adapter_thread_safe() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = Arc::new(SrplDispatcherAdapter::new(dispatcher));

    let mut handles = vec![];
    for i in 0..10 {
        let adapter_clone = Arc::clone(&adapter);
        let handle = std::thread::spawn(move || {
            let _adapter = adapter_clone;
            i
        });
        handles.push(handle);
    }

    let moved_count = handles.len();
    for handle in handles {
        handle.join().expect("adapter thread must not panic");
    }
    assert_eq!(moved_count, 10);
}
