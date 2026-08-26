//! Reading what the box has scheduled, without needing an `/etc` to read it from.
//!
//! The crontab fixtures are the real contents of `/etc/crontab` and `/etc/cron.d/*` on the
//! development box, tabs and all.

use std::fs;
use std::path::PathBuf;

mod support;

use rastro::collectors::cron::{
    CronCollector, CronFiles, CronTable, OwnerColumn, Schedule, crontab,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, items_of, object_of};
/// `/etc/crontab` as Debian ships it: two environment assignments and four tab-aligned jobs.
const SYSTEM_CRONTAB: &str = "\
# /etc/crontab: system-wide crontab
SHELL=/bin/sh
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

17 *\t* * *\troot\tcd / && run-parts --report /etc/cron.hourly
25 6\t* * *\troot\ttest -x /usr/sbin/anacron || { cd / && run-parts --report /etc/cron.daily; }
";

/// A real drop-in, including a command that contains an `=`.
const DROP_IN: &str = "\
30 3 * * 0 root test -e /run/systemd/system || SERVICE_MODE=1 /sbin/e2scrub_all -A -r
5-55/10 * * * * root command -v debian-sa1 > /dev/null && debian-sa1 1 1
";

fn tree(name: &str) -> PathBuf {
    scratch_tree(&format!("cron-{name}"), &[])
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn system(contents: &str) -> CronTable {
    crontab::parse(contents, OwnerColumn::Present).expect("this crontab is well formed")
}

#[test]
fn parse_reads_the_environment_a_crontab_sets() {
    // Act: `PATH` decides which binary a bare command name resolves to, so it is not
    // decoration.
    let table = system(SYSTEM_CRONTAB);

    // Assert
    assert_eq!(table.environment.len(), 2);
    let path = table
        .environment
        .iter()
        .find(|(name, _)| name.as_str() == "PATH")
        .expect("a PATH assignment");
    assert!(path.1.contains("/usr/local/sbin"));
}

#[test]
fn parse_reads_a_tab_aligned_job() {
    // Act: Debian aligns its crontabs with tabs, and taking five *fields* rather than a fixed
    // slice is what makes that parse the same as spaces.
    let table = system(SYSTEM_CRONTAB);

    // Assert
    let first = &table.jobs[0];
    assert_eq!(first.schedule.as_str(), "17 * * * *");
    assert_eq!(
        first.owner.as_ref().map(|owner| owner.as_str()),
        Some("root")
    );
    assert_eq!(
        first.command.as_str(),
        "cd / && run-parts --report /etc/cron.hourly"
    );
}

#[test]
fn parse_normalises_the_whitespace_inside_a_schedule_but_not_the_command() {
    // Act: reindenting a file must not show up as every job changing, while the command is
    // handed to a shell and must survive exactly.
    let table = system(SYSTEM_CRONTAB);

    // Assert
    assert_eq!(table.jobs[1].schedule.as_str(), "25 6 * * *");
    assert_eq!(
        table.jobs[1].command.as_str(),
        "test -x /usr/sbin/anacron || { cd / && run-parts --report /etc/cron.daily; }"
    );
}

#[test]
fn parse_does_not_take_a_command_containing_an_equals_for_an_assignment() {
    // Act: `SERVICE_MODE=1 /sbin/e2scrub_all` is a real command on this box, and splitting on
    // `=` would file the whole line as an environment variable.
    let table = system(DROP_IN);

    // Assert
    assert!(table.environment.is_empty());
    assert_eq!(table.jobs.len(), 2);
    assert!(table.jobs[0].command.as_str().contains("SERVICE_MODE=1"));
}

#[test]
fn parse_reads_a_schedule_with_a_step() {
    // Act
    let table = system(DROP_IN);

    // Assert
    assert_eq!(table.jobs[1].schedule.as_str(), "5-55/10 * * * *");
}

#[test]
fn parse_reads_a_user_crontab_without_an_account_column() {
    // Arrange: the dialect difference that matters. Reading this as a system crontab would
    // turn `/usr/bin/backup` into the account the job runs as.
    let contents = "0 4 * * * /usr/bin/backup --nightly\n";

    // Act
    let table = crontab::parse(contents, OwnerColumn::Absent).expect("well formed");

    // Assert
    assert_eq!(table.jobs[0].owner, None);
    assert_eq!(table.jobs[0].command.as_str(), "/usr/bin/backup --nightly");
}

#[test]
fn parse_reads_the_same_line_differently_in_the_two_dialects() {
    // Arrange: the same text, read both ways.
    let line = "0 4 * * * root /usr/bin/backup\n";

    // Act
    let as_system = crontab::parse(line, OwnerColumn::Present).expect("well formed");
    let as_user = crontab::parse(line, OwnerColumn::Absent).expect("well formed");

    // Assert
    assert_eq!(
        as_system.jobs[0].owner.as_ref().map(|owner| owner.as_str()),
        Some("root")
    );
    assert_eq!(as_system.jobs[0].command.as_str(), "/usr/bin/backup");
    assert_eq!(as_user.jobs[0].command.as_str(), "root /usr/bin/backup");
}

#[test]
fn parse_reads_a_shorthand_schedule() {
    // Act
    let table =
        crontab::parse("@daily root /usr/bin/thing\n", OwnerColumn::Present).expect("well formed");

    // Assert
    assert_eq!(table.jobs[0].schedule.as_str(), "@daily");
    assert_eq!(table.jobs[0].command.as_str(), "/usr/bin/thing");
}

#[test]
fn a_boot_schedule_is_marked_as_one() {
    // Act: `@reboot` is the one schedule that makes a job part of how the box comes up, which
    // puts it in the same class as an enabled unit.
    let at_boot = Schedule::new("@reboot").expect("a legal schedule");
    let daily = Schedule::new("@daily").expect("a legal schedule");
    let numeric = Schedule::new("0 4 * * *").expect("a legal schedule");

    // Assert
    assert!(at_boot.is_at_boot());
    assert!(!daily.is_at_boot());
    assert!(!numeric.is_at_boot());
}

#[test]
fn parse_refuses_a_line_with_too_few_schedule_fields() {
    // Act
    let result = crontab::parse("0 4 * root /usr/bin/thing\n", OwnerColumn::Present);

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_variable_set_twice() {
    // Act: cron takes the last, so the file is ambiguous about what every job in it runs with.
    let result = crontab::parse("PATH=/bin\nPATH=/usr/bin\n", OwnerColumn::Present);

    // Assert
    let failure = result.expect_err("an ambiguous file must not be resolved silently");
    assert!(
        failure.to_string().contains("PATH"),
        "the message must name the variable, got: {failure}"
    );
}

#[test]
fn read_reads_all_four_kinds_of_source() {
    // Arrange: the layout a Debian box really has.
    let root = tree("all-sources");
    write(&root, "etc/crontab", SYSTEM_CRONTAB);
    write(&root, "etc/cron.d/sysstat", DROP_IN);
    write(
        &root,
        "var/spool/cron/crontabs/operator",
        "0 4 * * * /usr/bin/backup\n",
    );
    write(&root, "etc/cron.daily/logrotate", "#!/bin/sh\n");
    write(&root, "etc/cron.daily/man-db", "#!/bin/sh\n");
    fs::create_dir_all(root.join("etc/cron.hourly")).expect("a writable directory");

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));
    let sources: Vec<String> = object_of(&observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect();

    // Assert: the drop-in and the system crontab by path, the user crontab by account, and
    // the four run-parts directories by path.
    assert!(sources.iter().any(|source| source.ends_with("etc/crontab")));
    assert!(
        sources
            .iter()
            .any(|source| source.ends_with("cron.d/sysstat"))
    );
    assert!(sources.contains(&"operator".to_owned()));
    assert!(sources.iter().any(|source| source.ends_with("cron.daily")));
}

#[test]
fn read_keys_a_user_crontab_by_the_account_rather_than_the_path() {
    // Arrange: a user crontab's filename *is* the account, and the spool path is an
    // implementation detail of cron's storage.
    let root = tree("spool-key");
    write(
        &root,
        "var/spool/cron/crontabs/operator",
        "@daily /usr/bin/x\n",
    );

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));

    // Assert
    let table = field(&observation, "operator");
    assert_eq!(items_of(&field(&table, "jobs")).len(), 1);
}

#[test]
fn read_lists_the_scripts_in_a_run_parts_directory() {
    // Arrange: these have no schedule of their own, so a script appearing here is a new
    // scheduled job with no schedule changing anywhere.
    let root = tree("run-parts");
    write(&root, "etc/cron.daily/apt-compat", "#!/bin/sh\n");
    write(&root, "etc/cron.daily/logrotate", "#!/bin/sh\n");

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));
    let daily = object_of(&observation)
        .into_iter()
        .find(|(key, _)| key.ends_with("cron.daily"))
        .map(|(_, value)| value)
        .expect("the daily directory");

    // Assert
    let scripts: Vec<String> = items_of(&field(&daily, "scripts"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(scripts, ["apt-compat", "logrotate"]);
}

#[test]
fn read_skips_a_placeholder_file_because_cron_does() {
    // Arrange: every Debian cron directory carries a `.placeholder`, and both cron and
    // `run-parts` require a name of letters, digits, underscores and hyphens.
    let root = tree("placeholder");
    write(&root, "etc/cron.daily/.placeholder", "");
    write(&root, "etc/cron.daily/logrotate", "#!/bin/sh\n");
    write(&root, "etc/cron.d/.placeholder", "");

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));
    let daily = object_of(&observation)
        .into_iter()
        .find(|(key, _)| key.ends_with("cron.daily"))
        .map(|(_, value)| value)
        .expect("the daily directory");

    // Assert: reporting one would put a job in the fingerprint the box does not run.
    assert_eq!(items_of(&field(&daily, "scripts")).len(), 1);
    assert!(
        !object_of(&observation)
            .into_iter()
            .any(|(key, _)| key.contains(".placeholder"))
    );
}

#[test]
fn read_skips_a_saved_copy_left_beside_a_real_drop_in() {
    // Arrange: the same name rule quietly covers the `.dpkg-old` and `.bak` files that
    // accumulate.
    let root = tree("saved-copy");
    write(&root, "etc/cron.d/sysstat", DROP_IN);
    write(
        &root,
        "etc/cron.d/sysstat.dpkg-old",
        "0 0 * * * root /bin/false\n",
    );

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));

    // Assert
    // Matched on the end of the key rather than anywhere in it: the scratch directory's own
    // path would otherwise satisfy a `contains`, which is how this test first passed for the
    // wrong reason.
    let keys: Vec<String> = object_of(&observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert!(
        !keys.iter().any(|key| key.ends_with(".dpkg-old")),
        "a saved copy must not be reported as a drop-in, got {keys:?}"
    );
    assert!(keys.iter().any(|key| key.ends_with("cron.d/sysstat")));
}

#[test]
fn read_reports_an_absent_source_as_absent_rather_than_omitting_it() {
    // Arrange: an empty tree, so nothing exists at all.
    let root = tree("nothing");

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));

    // Assert: `/etc/crontab` being absent is a fact about the box.
    let crontab = object_of(&observation)
        .into_iter()
        .find(|(key, _)| key.ends_with("etc/crontab"))
        .map(|(_, value)| value)
        .expect("the system crontab is reported either way");
    assert_eq!(crontab.content(), &Content::Scalar(Scalar::Null));
}

#[test]
fn read_tells_an_absent_run_parts_directory_apart_from_an_empty_one() {
    // Arrange: no `/etc/cron.weekly` at all is not the same as one with nothing in it.
    let root = tree("empty-vs-absent");
    fs::create_dir_all(root.join("etc/cron.weekly")).expect("a writable directory");

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));
    let entry = |suffix: &str| {
        object_of(&observation)
            .into_iter()
            .find(|(key, _)| key.ends_with(suffix))
            .map(|(_, value)| value)
            .expect("a reported directory")
    };

    // Assert
    assert_eq!(
        items_of(&field(&entry("cron.weekly"), "scripts")).len(),
        0,
        "an empty directory reports an empty script list"
    );
    assert_eq!(
        entry("cron.monthly").content(),
        &Content::Scalar(Scalar::Null),
        "an absent one reports null"
    );
}

#[test]
fn read_names_the_file_a_failure_came_from() {
    // Arrange
    let root = tree("named-failure");
    write(&root, "etc/cron.d/broken", "0 4 * root /usr/bin/thing\n");

    // Act
    let result = CronFiles::under(&root).read();

    // Assert
    let failure = result.expect_err("a malformed line must fail");
    assert!(
        failure.to_string().contains("broken"),
        "the message must name the file, got: {failure}"
    );
}

#[test]
fn read_keeps_the_jobs_in_the_files_order() {
    // Arrange: not a schedule, but it is how an operator reads the file.
    let root = tree("order");
    write(&root, "etc/crontab", SYSTEM_CRONTAB);

    // Act
    let observation = Observation::from(&CronFiles::under(&root).read().expect("well formed"));
    let crontab = object_of(&observation)
        .into_iter()
        .find(|(key, _)| key.ends_with("etc/crontab"))
        .map(|(_, value)| value)
        .expect("the system crontab");

    // Assert
    let schedules: Vec<String> = items_of(&field(&crontab, "jobs"))
        .iter()
        .map(|job| text(&field(job, "schedule")))
        .collect();
    assert_eq!(schedules, ["17 * * * *", "25 6 * * *"]);
}

#[test]
fn presence_is_always_present_because_the_subject_is_where_cron_keeps_jobs() {
    // Act & Assert: a box with cron uninstalled has every source absent, and the data says so
    // more precisely than an absent facet could.
    let root = tree("presence");
    assert_eq!(
        CronCollector::reading(CronFiles::under(&root)).presence(),
        Presence::Present
    );
}

#[test]
fn the_default_source_names_the_spool_and_the_four_run_parts_directories() {
    // Act & Assert: a smoke test on the real paths, since `under` rewrites them all.
    let files = CronFiles::new();
    let rendered = format!("{files:?}");
    for expected in [
        "/etc/crontab",
        "/etc/cron.d",
        "/var/spool/cron/crontabs",
        "/etc/cron.hourly",
        "/etc/cron.daily",
        "/etc/cron.weekly",
        "/etc/cron.monthly",
    ] {
        assert!(
            rendered.contains(expected),
            "{expected} must be among the sources, got: {rendered}"
        );
    }
}
