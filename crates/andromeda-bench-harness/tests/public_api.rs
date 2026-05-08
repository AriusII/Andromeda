use std::time::Instant;

use andromeda_bench_harness::{BenchmarkTempDir, BenchmarkTempFile, elapsed_micros};

#[test]
fn crate_root_exports_temp_helpers() {
    let _elapsed = elapsed_micros(Instant::now());

    fn assert_debug<T: std::fmt::Debug>() {}

    assert_debug::<BenchmarkTempDir>();
    assert_debug::<BenchmarkTempFile>();
}
