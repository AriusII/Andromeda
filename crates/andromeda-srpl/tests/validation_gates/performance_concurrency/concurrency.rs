use std::sync::Arc;

use andromeda_srpl_definition_batch::compile_narrow_procedure_signature;
use andromeda_srpl_lexer::lex;

#[test]
fn gate_07_concurrent_parsing() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool);";
    let srpl = Arc::new(srpl.to_string());

    let mut handles = vec![];

    for i in 0..10 {
        let srpl_clone = Arc::clone(&srpl);
        let handle = std::thread::spawn(move || {
            let result = compile_narrow_procedure_signature(&srpl_clone);
            assert!(result.is_ok(), "Parse failed in thread {}", i);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread panicked");
    }

    println!("Gate 07: Concurrent parsing thread-safe");
}

#[test]
fn gate_07_concurrent_lexing() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool);";
    let srpl = Arc::new(srpl.to_string());

    let mut handles = vec![];

    for i in 0..20 {
        let srpl_clone = Arc::clone(&srpl);
        let handle = std::thread::spawn(move || {
            let result = lex(&srpl_clone);
            assert!(result.is_ok(), "Lex failed in thread {}", i);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread panicked");
    }

    println!("Gate 07: Concurrent lexing thread-safe");
}
