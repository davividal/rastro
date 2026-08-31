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

/// A config body that seals every tree rastro's own table names, and the root.
///
/// **The cheap way to keep a real `filesystem` facet in a test.** Sealing the root stops the
/// walk of `/` at one entry, but a shipped rule for a tree *inside* it is more specific and
/// still descends — so the churn list has to be sealed by name as well. What comes back is one
/// entry per mount root: a facet that is genuinely `ok`, rendered through the real walk, for
/// about ten milliseconds instead of two minutes on a coverage-instrumented CI runner.
///
/// Prefer this over excluding the facet wherever a test looks at walked entries at all. It also
/// prints nothing on stderr, because a narrowing is not an exclusion and only an excluded
/// *collector* draws a WARN — which is what lets the "a clean run says nothing" tests use it.
///
/// If a shipped rule is ever added the list should learn it. The cost of forgetting is a slow
/// test, never a wrong one.
pub const SEALING_THE_SHIPPED_TREES: &str = r#"
[filesystem]
sealed = [
  "/",
  "/tmp",
  "/var/tmp",
  "/var/log",
  "/var/cache",
  "/var/lib/systemd/timesync",
  "/var/lib/systemd/random-seed",
]
"#;

/// The path of a config that seals every shipped tree, written once per process.
pub fn sealing_the_shipped_trees() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    PATH.get_or_init(|| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("sealed-walk-{}.toml", std::process::id()));
        std::fs::write(&path, SEALING_THE_SHIPPED_TREES).expect("a writable scratch directory");

        path.to_str().expect("a UTF-8 scratch path").to_owned()
    })
}
