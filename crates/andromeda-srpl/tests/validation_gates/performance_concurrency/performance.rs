use std::time::Instant;

use andromeda_srpl_lexer::lex;

use crate::support::compile_gate_source;

#[test]
fn gate_06_lexer_performance_baseline() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool, Timestamp i64, OrderId i64) \
        body { \
            read Inventory.ProductStock Stock one; \
            assert Quantity InsufficientStock; \
            update Inventory.ProductStock AvailableQuantity; \
            emit Reservation (Reserved, Timestamp, OrderId); \
        }";

    let start = Instant::now();
    for _ in 0..100 {
        let _ = lex(srpl);
    }
    let elapsed = start.elapsed() / 100;

    println!("  Lexer: {:.3}us per call", elapsed.as_micros());
    assert!(
        elapsed.as_micros() < 1000,
        "Lexer performance exceeded 1000us: {:.3}us",
        elapsed.as_micros()
    );

    println!("Gate 06: Lexer performance baseline met");
}

#[test]
fn gate_06_parser_performance_baseline() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool) \
        body { \
            read Inventory.ProductStock Stock one; \
            assert Quantity InsufficientStock; \
            update Inventory.ProductStock AvailableQuantity; \
            emit Reservation (Reserved); \
        }";

    let start = Instant::now();
    for _ in 0..100 {
        let _ = compile_gate_source(srpl, "performance baseline source should compile");
    }
    let elapsed = start.elapsed() / 100;

    println!(
        "  Full pipeline: {:.3}ms per call",
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(
        elapsed.as_millis() < 50,
        "Full pipeline exceeded 50ms: {:.3}ms",
        elapsed.as_secs_f64() * 1000.0
    );

    println!("Gate 06: Parser performance baseline met");
}
