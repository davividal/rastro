#![allow(dead_code)]

use std::path::Path;

/// The path of a config that excludes the filesystem walk.
///
/// **Written once per process, for speed rather than for coverage.** A walk of the whole host
/// costs seconds under a coverage-instrumented binary on a runner whose disk carries a cargo
/// registry and a target directory, and the tests invoke the binary dozens of times between
/// them. The ones that never read the `filesystem` facet skip it, which is the difference
/// between a suite measured in minutes and one measured in seconds.
///
/// It also removes a second problem: an instrumented run drops a `.profraw` file into the
/// target directory, which a walk of the whole host then reports — so a test that walks changes
/// what the next walk sees.
///
/// Keyed by process id because `cargo nextest` gives each test one, and several processes
/// writing a single path would let a reader catch a partial write.
pub fn without_walking() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    PATH.get_or_init(|| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("no-filesystem-walk-{}.toml", std::process::id()));
        std::fs::write(&path, "[collectors]\nexclude = [\"filesystem\"]\n")
            .expect("a writable scratch directory");

        path.to_str().expect("a UTF-8 scratch path").to_owned()
    })
}
