#[test]
fn procedure_store_production_sources_do_not_panic_or_unwrap() {
    let sources = [
        include_str!("../../procedure_store.rs"),
        include_str!("../decision.rs"),
        include_str!("../entry.rs"),
        include_str!("../evidence_role.rs"),
        include_str!("../registration.rs"),
        include_str!("../runtime.rs"),
        include_str!("../store.rs"),
    ];
    let forbidden = [
        "unwrap(",
        "unwrap_or(",
        "unwrap_or_else(",
        "expect(",
        "panic!",
        "todo!",
        "unimplemented!",
        "unreachable!",
    ];

    for source in sources {
        for token in forbidden {
            assert!(
                !source.contains(token),
                "procedure store production source must not contain panic/unwrap token: {token}"
            );
        }
    }
}

/// Doctrine guard: the Procedure Store must not expose an ad hoc SQL or
/// raw-text query surface. Callers must address procedures by typed id
/// or qualified name. This test scans this module's source for forbidden
/// tokens that would indicate such a surface was introduced. The needles
/// are built at runtime from halves so this test body is not itself a
/// false positive.
#[test]
fn procedure_store_exposes_no_ad_hoc_sql_surface() {
    let sources = [
        include_str!("../../procedure_store.rs"),
        include_str!("../decision.rs"),
        include_str!("../entry.rs"),
        include_str!("../registration.rs"),
        include_str!("../store.rs"),
    ];
    let halves: &[(&str, &str)] = &[
        ("fn query_", "sql"),
        ("fn execute_", "sql"),
        ("fn raw_", "query"),
        ("raw_", "sql"),
        ("SE", "LECT "),
        ("INSERT ", "INTO"),
        ("EXECUTE ", "IMMEDIATE"),
        ("prepare_", "sql"),
    ];
    for source in sources {
        for (a, b) in halves {
            let needle = format!("{a}{b}");
            assert!(
                !source.contains(needle.as_str()),
                "procedure store source must not expose ad-hoc SQL surface token: {needle}"
            );
        }
    }
}
