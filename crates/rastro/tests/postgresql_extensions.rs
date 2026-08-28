//! Reading the extensions installed in one database.
//!
//! The fixture is shaped like a real cluster's, which is the finding as much as the
//! mechanism: monitoring extension *packages* are installed but no extension is created, so
//! `plpgsql` is all there is until somebody restarts the server.

mod support;

use rastro::collectors::postgresql::PsqlExtensions;
use rastro_collector::Observation;
use support::observation::{field, keys_of, text};

/// The three columns the collector's query asks for, in order.
const EXTENSIONS: &str = "\
plpgsql,1.0,pg_catalog
pg_stat_statements,1.11,public
";

fn rendered(csv: &str) -> Observation {
    Observation::from(&PsqlExtensions::parse(csv).expect("this output is well formed"))
}

#[test]
fn parse_reads_an_extension_with_its_version_and_schema() {
    // Act
    let extensions = rendered(EXTENSIONS);

    // Assert: the version is what an upgrade changes without touching anything else, and
    // the schema is where the extension's objects actually live.
    let statements = field(&extensions, "pg_stat_statements");
    assert_eq!(text(&field(&statements, "version")), "1.11");
    assert_eq!(text(&field(&statements, "schema")), "public");
}

#[test]
fn parse_orders_extensions_by_name() {
    // Act
    let extensions = rendered(EXTENSIONS);

    // Assert
    assert_eq!(keys_of(&extensions), vec!["pg_stat_statements", "plpgsql"]);
}

#[test]
fn parse_reads_a_database_with_no_extensions() {
    // Act
    let extensions = PsqlExtensions::parse("\n").expect("no extensions is an ordinary answer");

    // Assert: a database somebody dropped `plpgsql` from has none, which is state rather
    // than a failed read.
    assert!(extensions.extensions().is_empty());
}

#[test]
fn parse_refuses_a_row_with_the_wrong_number_of_columns() {
    // Act
    let refused = PsqlExtensions::parse("plpgsql,1.0\n");

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_the_same_extension_twice() {
    // Arrange: one extension cannot be installed twice in a database.
    let contradiction = "\
plpgsql,1.0,pg_catalog
plpgsql,1.1,public
";

    // Act
    let refused = PsqlExtensions::parse(contradiction);

    // Assert
    assert!(refused.is_err());
}
