//! rastro's include resolution against nginx's own answer.
//!
//! Every other test here asserts what rastro does with a rule somebody measured once, which
//! pins the code and cannot catch a rule that was written down wrong. This one asks nginx.
//! `nginx -T` prints a `# configuration file <path>:` marker for every file it read, so the
//! two lists are directly comparable, and the awkward cases live in one tree: a relative
//! glob, an absolute include, a nested include, a symlinked one, a glob that matches nothing,
//! and a file pulled in twice.
//!
//! **This is the one place `-T` is allowed, and the collector still never runs it.** Testing
//! a configuration creates every log file it names, which is why the shipped binary reads the
//! files itself — see `docs/decisions.md`. Here the mutation is contained twice over: the run
//! is given a prefix inside this test's own scratch tree, `-e stderr` leaves it no error log
//! to create, and the fixture names its own `pid` file so the run touches nothing outside
//! the tree. All three were measured, the last of them by the unprivileged half of the
//! container suite failing on `/run/nginx.pid`.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

use rastro::collectors::nginx::source::ConfigurationFiles;
use rastro::collectors::nginx::value_objects::ConfigurationSource;
use support::fs_tree::{scratch_tree, write};

const PROGRAM: &str = "nginx";

/// What nginx prints in front of every file it read.
const MARKER: &str = "# configuration file ";

/// The end of that line, which is a colon rather than the end of the path.
const MARKER_END: char = ':';

/// The top-level file, exercising every shape of include in one place.
///
/// The included files are empty on purpose: which files nginx reads is the question, and
/// giving them directives would only add ways for the fixture to be rejected for reasons
/// that are not about resolution.
const ROOT: &str = "\
# Its own pid file, inside the prefix this test hands nginx. Measured: a configuration
# test opens the pid path, so without this line the unprivileged half of the container
# suite fails on the compiled-in `/run/nginx.pid` rather than on anything rastro did.
pid nginx.pid;
events { }
http {
    include conf.d/*.conf;
    include shared/common.conf;
    include shared/nested.conf;
    include empty.d/*.conf;
    include sites-enabled/*;
}
";

fn tree() -> PathBuf {
    let prefix = scratch_tree(
        "nginx-conformance",
        &[
            "conf.d",
            "shared",
            "empty.d",
            "sites-available",
            "sites-enabled",
        ],
    );

    write(&prefix, "nginx.conf", ROOT);
    // Written out of alphabetical order, because glob(3) sorts and a directory does not.
    for name in ["c_third", "a_first", "b_second"] {
        write(
            &prefix,
            &format!("conf.d/{name}.conf"),
            "# a file nginx reads\n",
        );
    }
    // A file that includes another, so the depth-first order is exercised rather than
    // assumed, and one that is pulled in twice from two branches.
    write(
        &prefix,
        "shared/common.conf",
        "include shared/nested.conf;\n",
    );
    write(&prefix, "shared/nested.conf", "# shared\n");
    write(&prefix, "sites-available/site.conf", "# a site\n");
    symlink(
        prefix.join("sites-available/site.conf"),
        prefix.join("sites-enabled/site.conf"),
    )
    .expect("a writable scratch tree");

    prefix
}

/// The files nginx says it read, in the order it read them.
fn nginx_read(prefix: &Path) -> Vec<String> {
    let configuration = prefix.join("nginx.conf");
    let run = Command::new(PROGRAM)
        .args(["-p", &prefix.to_string_lossy()])
        .args(["-c", &configuration.to_string_lossy()])
        // No error log to create: the one file `-T` would otherwise leave behind.
        .args(["-e", "stderr"])
        .arg("-T")
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "this test asks nginx which files it reads, and {PROGRAM} could not be run: \
                 {error}. Install it (`apt-get install nginx`, `apk add nginx`, \
                 `brew install nginx`) or run the suite through scripts/test-in-container.sh"
            )
        });

    assert!(
        run.status.success(),
        "nginx refused the fixture: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    String::from_utf8_lossy(&run.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix(MARKER))
        .map(|line| line.trim_end_matches(MARKER_END).to_owned())
        .collect()
}

/// The files rastro says nginx would read, in the order it resolved them.
fn rastro_read(prefix: &Path) -> Vec<String> {
    ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read()
    .files
    .iter()
    .map(|file| file.path.as_str().to_owned())
    .collect()
}

#[test]
fn rastro_reads_the_files_nginx_reads_in_the_order_nginx_reads_them() {
    // Arrange
    let prefix = tree();

    // Act
    let nginx = nginx_read(&prefix);
    let rastro = rastro_read(&prefix);

    // Assert: the whole list, ordered. A glob's order, a nested include's depth and a file
    // pulled in twice are all in here, and any of them differing is a divergence from the
    // only authority on the question.
    assert_eq!(rastro, nginx);
}
