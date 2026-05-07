pub(super) fn print_audit_help() {
    println!("Andromeda audit administration commands");
    println!();
    println!("USAGE: andromeda-cli audit <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!(
        "  inspect  Inspect durable audit journal replay through the trace inspection contract"
    );
    println!("  compact  Rewrite a durable audit journal through the retention contract");
    println!("  verify   Verify durable audit journal checksums without creating missing files");
    println!();
    println!("INSPECT OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to replay");
    println!("  --trace-id <u128>         Filter by trace id");
    println!("  --principal <id>          Filter by durable principal id");
    println!("  --family <family>         Filter by trace family");
    println!("  --lsn-range <start..end>  Inclusive LSN filter range");
    println!("  --lsn-start <lsn>         Inclusive LSN filter start");
    println!("  --lsn-end <lsn>           Inclusive LSN filter end");
    println!("  --limit <n>               Max rows; 1..=TRACE_QUERY_MAX_LIMIT");
    println!("  --offset <n>              Rows to skip after filtering");
    println!("  --include-total-count     Include full matching count");
    println!("  --json                    Emit machine-readable JSON output");
    println!("  --diagnostic-json         Emit replay evidence fields in diagnostic JSON");
    println!();
    println!("COMPACT OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to rewrite");
    println!("  --retain-from-lsn <lsn>   Keep records at or after this record LSN");
    println!("  --preserve-forensic-hold  Preserve forensic hold records below the LSN floor");
    println!("  --diagnostic-json         Emit compaction report as diagnostic JSON");
    println!();
    println!("VERIFY OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to verify");
    println!("  --json                    Emit verification report as JSON");
    println!("  --diagnostic-json         Emit verification report as diagnostic JSON");
    println!("  -h, --help                Show this help message");
}
