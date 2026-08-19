//! Reading a package manager, without needing one installed.
//!
//! The fixtures are real output: `dpkg-query` from `debian:stable-slim` (dpkg 1.22.22) and
//! `/lib/apk/db/installed` from `alpine:latest` (apk-tools 3.0.6).

use rastro::collectors::packages::{
    ApkDatabase, DpkgQuery, Package, PackageManager, PackageName, PackageSet, PackagesCollector,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation};

/// Six tab-separated fields, the format rastro asks dpkg for.
const DPKG_OUTPUT: &str = "\
apt\t3.0.3\tarm64\tinstall\tinstalled\tok
debconf\t1.5.91\tall\tinstall\tinstalled\tok
oldconf\t1.0\tarm64\tdeinstall\tconfig-files\tok
gone\t2.0\tarm64\tpurge\tnot-installed\tok
";

/// Two stanzas, trimmed to the three fields rastro reads out of the dozen apk writes.
const APK_DATABASE: &str = "\
C:Q17hOhjufinXWHIBdAPVnASE2s2WM=
P:alpine-baselayout
V:3.7.2-r1
A:aarch64
S:8265
T:Alpine base dir structure and init scripts

C:Q1x1B2rj8xxxxxxxxxxxxxxxxxxxxx=
P:busybox
V:1.37.0-r18
A:aarch64
";

fn dpkg(output: &str) -> PackageSet {
    DpkgQuery::parse(output).expect("this output is well formed")
}

fn apk(database: &str) -> PackageSet {
    ApkDatabase::parse(database).expect("this database is well formed")
}

fn package<'a>(set: &'a PackageSet, name: &str) -> &'a Package {
    let name = PackageName::new(name).expect("a legal package name");
    set.packages()
        .get(&name)
        .unwrap_or_else(|| panic!("expected a {name:?} package"))
}

#[test]
fn dpkg_reads_the_fields_that_describe_a_package() {
    // Act
    let set = dpkg(DPKG_OUTPUT);

    // Assert
    let package = package(&set, "apt");
    assert_eq!(package.version.as_str(), "3.0.3");
    assert_eq!(package.architecture.as_str(), "arm64");
}

#[test]
fn dpkg_reads_the_status_as_three_words() {
    // Act
    let set = dpkg(DPKG_OUTPUT);

    // Assert: dpkg decodes its own vocabulary, so rastro maintains no alphabet of status
    // letters and a diff reads `installed` rather than `ii`.
    let status = package(&set, "apt")
        .status
        .as_ref()
        .expect("dpkg reports a status");
    assert_eq!(status.selection.as_str(), "install");
    assert_eq!(status.state.as_str(), "installed");
    assert_eq!(status.error_flag.as_str(), "ok");
}

#[test]
fn dpkg_keeps_a_package_it_knows_but_has_not_installed() {
    // Act
    let set = dpkg(DPKG_OUTPUT);

    // Assert: absence is state. Removed-but-configured and purged are a real difference
    // between two runs, so neither is dropped for not being installed.
    let removed = package(&set, "oldconf")
        .status
        .as_ref()
        .expect("dpkg reports a status");
    assert_eq!(removed.state.as_str(), "config-files");
    let purged = package(&set, "gone")
        .status
        .as_ref()
        .expect("dpkg reports a status");
    assert_eq!(purged.state.as_str(), "not-installed");
}

#[test]
fn dpkg_keys_the_set_by_name() {
    // Act
    let set = dpkg(DPKG_OUTPUT);

    // Assert
    assert_eq!(
        set.packages()
            .keys()
            .map(PackageName::as_str)
            .collect::<Vec<&str>>(),
        ["apt", "debconf", "gone", "oldconf"]
    );
}

#[test]
fn dpkg_refuses_a_line_with_the_wrong_field_count() {
    // Act
    let result = DpkgQuery::parse("apt\t3.0.3\tarm64\n");

    // Assert: output rastro cannot read is reported, never quietly skipped.
    assert!(result.is_err());
}

#[test]
fn dpkg_refuses_a_package_reported_twice() {
    // Act: dpkg cannot produce this, so it means the output was misread.
    let result = DpkgQuery::parse(
        "apt\t3.0.3\tarm64\tinstall\tinstalled\tok\napt\t3.0.3\tarm64\tinstall\tinstalled\tok\n",
    );

    // Assert
    assert!(result.is_err());
}

#[test]
fn apk_reads_the_three_fields_it_needs_from_a_stanza() {
    // Act
    let set = apk(APK_DATABASE);

    // Assert: the other fields apk writes describe the package, not the host's state.
    let package = package(&set, "alpine-baselayout");
    assert_eq!(package.version.as_str(), "3.7.2-r1");
    assert_eq!(package.architecture.as_str(), "aarch64");
}

#[test]
fn apk_reports_no_status_rather_than_inventing_one() {
    // Act
    let set = apk(APK_DATABASE);

    // Assert: apk's database lists what is installed and nothing else, so there is no
    // desired-versus-actual state to report.
    assert!(package(&set, "busybox").status.is_none());
}

#[test]
fn apk_reads_every_stanza() {
    // Act
    let set = apk(APK_DATABASE);

    // Assert
    assert_eq!(
        set.packages()
            .keys()
            .map(PackageName::as_str)
            .collect::<Vec<&str>>(),
        ["alpine-baselayout", "busybox"]
    );
}

#[test]
fn apk_refuses_a_stanza_with_no_version() {
    // Act
    let result = ApkDatabase::parse("P:busybox\nA:aarch64\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn apk_reads_a_database_on_disk() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-apk-installed");
    std::fs::write(&path, APK_DATABASE).expect("the temp directory should be writable");

    // Act
    let set = ApkDatabase::at(&path)
        .read()
        .expect("this fixture is well formed");

    // Assert
    assert_eq!(set.packages().len(), 2);
}

#[test]
fn apk_names_the_database_it_could_not_open() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-apk-absent");

    // Act
    let failure = ApkDatabase::at(&path)
        .read()
        .expect_err("a missing database is a failure");

    // Assert
    assert!(
        failure.to_string().contains(&path.display().to_string()),
        "got {failure}"
    );
}

#[test]
fn presence_is_absent_when_the_host_has_no_manager_rastro_can_read() {
    // Arrange
    let collector = PackagesCollector::reading(None, None);

    // Act & Assert: the first genuine `absent` rastro produces. A box with neither manager
    // is a box whose packages are not managed by either, which is state, not a failure.
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_present_when_one_manager_is_found() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-apk-present");
    std::fs::write(&path, APK_DATABASE).expect("the temp directory should be writable");

    // Act & Assert
    let collector = PackagesCollector::reading(None, Some(ApkDatabase::at(&path)));
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_inventory_is_keyed_by_manager() {
    // Arrange: a box carrying both needs no arbitrary precedence, and the two shapes differ
    // honestly because only dpkg reports a status.
    let path = std::env::temp_dir().join("rastro-apk-both");
    std::fs::write(&path, APK_DATABASE).expect("the temp directory should be writable");
    let collector = PackagesCollector::reading(None, Some(ApkDatabase::at(&path)));

    // Act
    let observation = collector.collect().expect("this fixture is well formed");

    // Assert
    let Content::Object(managers) = observation.content() else {
        panic!("the inventory renders as an object keyed by manager");
    };
    assert_eq!(
        managers.keys().map(String::as_str).collect::<Vec<&str>>(),
        [PackageManager::Apk.as_str()]
    );
}

#[test]
fn the_observation_of_a_package_carries_the_contracted_keys() {
    // Arrange: the key names are the output contract, so one test reads the rendered shape.
    let set = dpkg(DPKG_OUTPUT);

    // Act
    let observation = Observation::from(package(&set, "apt"));

    // Assert
    let Content::Object(entries) = observation.content() else {
        panic!("a package renders as an object");
    };
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<&str>>(),
        ["architecture", "status", "version"]
    );
}

#[test]
fn a_package_with_no_status_omits_the_key_rather_than_nulling_it() {
    // Arrange: an absent key says "this manager does not report one", where a null would
    // read as "reported, and empty".
    let set = apk(APK_DATABASE);

    // Act
    let observation = Observation::from(package(&set, "busybox"));

    // Assert
    let Content::Object(entries) = observation.content() else {
        panic!("a package renders as an object");
    };
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<&str>>(),
        ["architecture", "version"]
    );
}
