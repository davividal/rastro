//! Reading the host's timekeeping, without needing an `/etc` to read it from.
//!
//! This collector reads files rather than running `timedatectl`, because that tool starts a
//! systemd unit on the box being fingerprinted. The collector's own documentation carries the
//! measurement; `reading_the_files_starts_nothing` below is the property that matters.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::time::{ClockFiles, ClockSettings, TimeCollector};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};

fn tree(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("time-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("etc")).expect("a writable scratch directory");

    root
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("a writable tree");
    fs::write(path, contents).expect("a writable file");
}

/// A `/etc/localtime` symlink into a zoneinfo database under the same scratch root.
fn link_localtime(root: &Path, zone: &str) {
    let target = root.join(format!("usr/share/zoneinfo/{zone}"));
    fs::create_dir_all(target.parent().expect("a parent")).expect("a writable tree");
    fs::write(&target, "").expect("a writable zone file");
    std::os::unix::fs::symlink(&target, root.join("etc/localtime")).expect("a creatable symlink");
}

fn read(root: &Path) -> ClockSettings {
    ClockFiles::under(root)
        .read()
        .expect("this tree is well formed")
}

fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn field(observation: &Observation, name: &str) -> Observation {
    object_of(observation)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("expected a {name:?} field"))
}

fn keys_of(observation: &Observation) -> Vec<String> {
    object_of(observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

#[test]
fn read_takes_the_timezone_from_the_localtime_symlink() {
    // Arrange: the symlink is what every program resolving a local time follows.
    let root = tree("symlink");
    link_localtime(&root, "Europe/Berlin");

    // Act
    let settings = read(&root);

    // Assert
    assert_eq!(
        settings.timezone.as_ref().map(|zone| zone.as_str()),
        Some("Europe/Berlin")
    );
}

#[test]
fn read_prefers_the_symlink_over_the_debian_text_file_when_they_disagree() {
    // Arrange: `/etc/timezone` is a Debian convenience that `dpkg-reconfigure` keeps in step
    // and a hand edit does not, so the two really can disagree.
    let root = tree("disagreement");
    link_localtime(&root, "Etc/UTC");
    write(&root, "etc/timezone", "Europe/Berlin\n");

    // Act
    let settings = read(&root);

    // Assert: the zone the box *is* in, not the one it is documented to be in.
    assert_eq!(
        settings.timezone.as_ref().map(|zone| zone.as_str()),
        Some("Etc/UTC")
    );
}

#[test]
fn read_falls_back_to_the_text_file_when_there_is_no_symlink() {
    // Arrange
    let root = tree("text-fallback");
    write(&root, "etc/timezone", "Etc/UTC\n");

    // Act
    let settings = read(&root);

    // Assert
    assert_eq!(
        settings.timezone.as_ref().map(|zone| zone.as_str()),
        Some("Etc/UTC")
    );
}

#[test]
fn read_reports_no_timezone_when_the_host_names_none() {
    // Arrange: an empty tree, which leaves every program on the box in UTC.
    let root = tree("no-zone");

    // Act
    let settings = read(&root);

    // Assert
    assert_eq!(settings.timezone, None);
}

#[test]
fn read_reads_a_hardware_clock_running_on_local_time() {
    // Arrange: the third line is the scale. The first two move on their own and are not read.
    let root = tree("local-clock");
    write(
        &root,
        "etc/adjtime",
        "0.0 1755000000 0.0\n1755000000\nLOCAL\n",
    );

    // Act
    let settings = read(&root);

    // Assert: `true` here makes every timestamp ambiguous twice a year.
    assert!(settings.local_real_time_clock);
}

#[test]
fn read_reads_a_hardware_clock_running_on_utc() {
    // Arrange
    let root = tree("utc-clock");
    write(
        &root,
        "etc/adjtime",
        "0.0 1755000000 0.0\n1755000000\nUTC\n",
    );

    // Act & Assert
    assert!(!read(&root).local_real_time_clock);
}

#[test]
fn read_treats_an_absent_adjtime_as_a_clock_on_utc() {
    // Arrange: `hwclock` writes the file only once it has something to record, so a box that
    // has never been told otherwise runs its hardware clock on UTC. The development box has
    // no such file at all.
    let root = tree("no-adjtime");

    // Act
    let settings = read(&root);

    // Assert: treating the absence as unknown would make the common case a null.
    assert!(!settings.local_real_time_clock);
}

#[test]
fn read_reports_synchronisation_from_the_stamp_file() {
    // Arrange: existence is the fact. The file is empty and its mtime moves with every
    // synchronisation, so only whether it is there is state.
    let root = tree("synchronised");
    write(&root, "run/systemd/timesync/synchronized", "");

    // Act & Assert
    assert!(read(&root).synchronised);
}

#[test]
fn read_reports_no_synchronisation_when_the_stamp_is_absent() {
    // Arrange
    let root = tree("unsynchronised");

    // Act & Assert
    assert!(!read(&root).synchronised);
}

#[test]
fn reading_the_files_starts_nothing() {
    // Arrange: the property this collector was rewritten for. `timedatectl` activates
    // `systemd-timedated.service`, which then appears in the next run's `processes` facet and
    // breaks the determinism harness. A file read cannot activate anything, and the way to
    // pin that here is to show the source names no executable at all.
    let root = tree("no-exec");
    write(&root, "etc/timezone", "Etc/UTC\n");

    // Act
    let source = ClockFiles::under(&root);
    let rendered = format!("{source:?}");

    // Assert: four paths and no tool. If a future edit reintroduced `timedatectl`, the source
    // would have to hold a `CanonicalTool` and this would say so.
    assert!(
        !rendered.contains("CanonicalTool") && !rendered.contains("timedatectl"),
        "this collector must not run anything, got: {rendered}"
    );
    assert!(read(&root).timezone.is_some());
}

#[test]
fn every_value_survives_the_diffable_view() {
    // Arrange: the earlier version of this facet carried two clock readings, which were
    // volatile and never reached a diff. Nothing here is a clock any more.
    let root = tree("diffable");
    link_localtime(&root, "Etc/UTC");
    write(&root, "run/systemd/timesync/synchronized", "");

    // Act
    let observation = Observation::from(&read(&root));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert
    assert_eq!(
        keys_of(&diffable),
        ["local_real_time_clock", "synchronised", "timezone"]
    );
    assert_eq!(
        field(&diffable, "timezone").content(),
        &Content::Scalar(Scalar::Text("Etc/UTC".to_owned()))
    );
}

#[test]
fn losing_synchronisation_stays_visible_in_a_diff() {
    // Arrange: a box that loses sync and does not regain it is a fault worth seeing, not
    // noise, which is why this is not volatile.
    let root = tree("lost-sync");
    link_localtime(&root, "Etc/UTC");

    // Act
    let diffable = Observation::from(&read(&root))
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    assert_eq!(
        field(&diffable, "synchronised").content(),
        &Content::Scalar(Scalar::Boolean(false))
    );
}

#[test]
fn presence_is_always_present_because_every_host_keeps_time_somehow() {
    // Act & Assert: a change from the `undetermined` the `timedatectl` version gave when the
    // tool was missing. A box with none of these files is not a box rastro cannot see, it is
    // a box in UTC with no zone configured.
    let root = tree("presence");
    assert_eq!(
        TimeCollector::reading(ClockFiles::under(&root)).presence(),
        Presence::Present
    );
}

#[test]
fn collect_reports_the_facet_even_on_a_host_with_none_of_the_files() {
    // Arrange
    let root = tree("bare");

    // Act
    let collected = TimeCollector::reading(ClockFiles::under(&root))
        .collect()
        .expect("absent files are not a failure");

    // Assert
    assert_eq!(
        field(&collected, "timezone").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn the_collector_reports_its_second_version() {
    // Act: the source changed from `timedatectl` to the files and two of the five fields went
    // with it, so a consumer comparing fingerprints across the change needs to see that the
    // collector moved rather than the host.
    let collector = TimeCollector::reading(ClockFiles::new());

    // Assert
    assert_eq!(collector.identity().version.as_str(), "2");
}
