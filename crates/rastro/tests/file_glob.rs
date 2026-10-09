//! Matching a pattern against a directory's names, as `glob(3)` does, in time a hostile name or
//! pattern cannot stretch.

use std::time::{Duration, Instant};

use rastro::collectors::file_glob::matching;

mod support;

use support::fs_tree::{scratch_tree, write};

fn names_matching(name: &str, files: &[&str], pattern: &str) -> Vec<String> {
    let root = scratch_tree(name, &[]);
    for file in files {
        write(&root, file, "");
    }

    matching(&root.join(pattern))
        .expect("a pattern rastro understands")
        .iter()
        .map(|path| {
            path.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn a_star_matches_any_run_and_a_question_mark_exactly_one() {
    // Act & Assert
    assert_eq!(
        names_matching(
            "glob-star",
            &["a.conf", "ab.conf", "abc.conf", "b.txt"],
            "a?*.conf"
        ),
        ["ab.conf", "abc.conf"]
    );
}

#[test]
fn a_star_matches_an_empty_run_and_a_literal_must_match_exactly() {
    // Act & Assert
    assert_eq!(
        names_matching(
            "glob-empty-run",
            &["x.conf", "xy.conf", "y.conf"],
            "x*.conf"
        ),
        ["x.conf", "xy.conf"]
    );
}

#[test]
fn a_leading_dot_is_matched_only_by_a_pattern_that_spells_one() {
    // Act & Assert: `glob(3)`'s rule, which keeps an editor's `.site.conf.swp` out.
    assert_eq!(
        names_matching("glob-dot", &[".hidden.conf", "seen.conf"], "*.conf"),
        ["seen.conf"]
    );
}

#[test]
fn a_pattern_of_many_stars_matches_in_time_a_hostile_name_cannot_stretch() {
    // Arrange: the pattern can come from a file another account owns, a redis `include` among
    // them, and a matcher that tries every split for every star is exponential in the stars.
    let name = "a".repeat(60);

    // Act
    let started = Instant::now();
    let matched = names_matching("glob-stars", &[&name], &format!("{}b", "*a".repeat(20)));

    // Assert: no match, and answered at once.
    assert!(matched.is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
