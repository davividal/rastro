//! Reading what a cluster could install, without a cluster to read it from.
//!
//! `pg_available_extensions` reads the extension control files from disk, so `default_version`
//! is cluster-wide while `installed_version` belongs to the database that answered. The gap
//! between the two is the pending upgrade a restart or an `ALTER EXTENSION` would take.

use rastro::collectors::postgresql::{AvailableExtension, ExtensionName, PsqlAvailableExtensions};

/// The three columns the collector's query asks for: one installed, one merely available.
const AVAILABLE: &str = "\
plpgsql,1.0,1.0
pg_stat_statements,1.11,
";

fn parsed(csv: &str) -> Vec<AvailableExtension> {
    PsqlAvailableExtensions::parse(csv)
        .expect("this output is well formed")
        .extensions()
        .to_vec()
}

fn named<'a>(extensions: &'a [AvailableExtension], name: &str) -> &'a AvailableExtension {
    extensions
        .iter()
        .find(|extension| extension.name == ExtensionName::new(name).expect("a legal name"))
        .expect("the fixture has this extension")
}

#[test]
fn parse_reads_an_installed_extension() {
    // Act
    let extensions = parsed(AVAILABLE);

    // Assert
    let plpgsql = named(&extensions, "plpgsql");
    assert_eq!(plpgsql.default_version, "1.0");
    assert_eq!(plpgsql.installed_version.as_deref(), Some("1.0"));
}

#[test]
fn parse_reads_an_available_but_uninstalled_extension() {
    // Act
    let extensions = parsed(AVAILABLE);

    // Assert: an empty installed version means the extension is installable but not created
    // in the database that answered, which is a real state rather than a missing read.
    let available = named(&extensions, "pg_stat_statements");
    assert_eq!(available.default_version, "1.11");
    assert_eq!(available.installed_version, None);
}

#[test]
fn parse_reads_a_cluster_with_no_available_extensions_as_empty() {
    // Act & Assert: a build with no control files is a real, if unusual, state.
    assert!(
        PsqlAvailableExtensions::parse("")
            .expect("empty is well formed")
            .extensions()
            .is_empty()
    );
}

#[test]
fn new_refuses_one_extension_available_twice() {
    // Act & Assert: pg_available_extensions reads one control file per name, so a repeat
    // means two reads were spliced.
    let repeated = "plpgsql,1.0,1.0\nplpgsql,1.0,\n";
    assert!(PsqlAvailableExtensions::parse(repeated).is_err());
}
