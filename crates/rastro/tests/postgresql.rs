//! Reading a cluster's effective settings, without needing a cluster to read them from.
//!
//! Every fixture here is output psql really produced on a PostgreSQL 17 cluster, which is
//! why the odd-looking ones are in: `"""$user"", public"` is how `search_path` arrives,
//! and `log_line_prefix` really does end in a space.

use rastro::collectors::postgresql::{PsqlSettings, Setting, SettingName, SettingSource};

/// The six columns the collector asks for, as psql renders them with `--csv -t`.
const SETTINGS: &str = "\
DateStyle,\"ISO, MDY\",,configuration file,user,f
log_line_prefix,%m [%p] %q%u@%d ,,configuration file,sighup,f
max_connections,100,,configuration file,postmaster,f
search_path,\"\"\"$user\"\", public\",,default,user,f
shared_buffers,16384,8kB,configuration file,postmaster,f
";

fn parsed(csv: &str) -> Vec<Setting> {
    PsqlSettings::parse(csv)
        .expect("this output is well formed")
        .settings()
        .to_vec()
}

fn named<'a>(settings: &'a [Setting], name: &str) -> &'a Setting {
    settings
        .iter()
        .find(|setting| setting.name == SettingName::new(name).expect("a legal name"))
        .expect("the fixture has this setting")
}

#[test]
fn parse_reads_every_column_of_a_setting() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert
    let shared_buffers = named(&settings, "shared_buffers");
    assert_eq!(shared_buffers.value.as_str(), "16384");
    assert_eq!(
        shared_buffers.unit.as_ref().map(|unit| unit.as_str()),
        Some("8kB")
    );
    assert_eq!(
        shared_buffers.source,
        SettingSource::new("configuration file").expect("a legal source")
    );
    assert!(!shared_buffers.pending_restart);
}

#[test]
fn parse_keeps_a_quoted_value_whole() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: the comma inside `ISO, MDY` is part of the value, not a column break.
    assert_eq!(named(&settings, "DateStyle").value.as_str(), "ISO, MDY");
}

#[test]
fn parse_reads_a_doubled_quote_as_one_quote() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: RFC 4180 escaping, which is how `search_path` arrives every time.
    assert_eq!(
        named(&settings, "search_path").value.as_str(),
        "\"$user\", public"
    );
}

#[test]
fn parse_preserves_trailing_space_in_a_value() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: trimming would change what the server would actually log, and the
    // operator who put that space there meant it.
    assert_eq!(
        named(&settings, "log_line_prefix").value.as_str(),
        "%m [%p] %q%u@%d "
    );
}

#[test]
fn parse_reads_an_empty_unit_as_no_unit() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: 303 of a cluster's 379 settings have no unit, and psql renders both an
    // empty string and a null as nothing at all.
    assert_eq!(named(&settings, "max_connections").unit, None);
}

#[test]
fn parse_reads_a_setting_awaiting_a_restart() {
    // Arrange
    let changed = "max_connections,200,,configuration file,postmaster,t\n";

    // Act
    let settings = parsed(changed);

    // Assert: a value edited in postgresql.conf that the running server has not taken
    // up yet is exactly the drift this collector exists to show.
    assert!(named(&settings, "max_connections").pending_restart);
}

#[test]
fn parse_drops_a_setting_that_came_from_our_own_connection() {
    // Arrange: psql sets `application_name` on the connection the collector opens, so
    // the server reports it with source `client`.
    let ours = "\
application_name,psql,,client,user,f
max_connections,100,,configuration file,postmaster,f
";

    // Act
    let settings = parsed(ours);

    // Assert: recording it would put rastro's own connection into the fingerprint and
    // call it the state of the host.
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].name.as_str(), "max_connections");
}

#[test]
fn parse_reads_a_value_containing_a_newline() {
    // Arrange: `archive_command` holds a shell command, so nothing stops it spanning
    // lines. A row-per-line parser would read this as two broken rows.
    let multiline = "archive_command,\"test ! -f x &&\ncp %p x\",,configuration file,sighup,f\n";

    // Act
    let settings = parsed(multiline);

    // Assert
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].value.as_str(), "test ! -f x &&\ncp %p x");
}

#[test]
fn parse_orders_settings_by_name() {
    // Arrange
    let unsorted = "\
shared_buffers,16384,8kB,configuration file,postmaster,f
DateStyle,\"ISO, MDY\",,configuration file,user,f
max_connections,100,,configuration file,postmaster,f
";

    // Act
    let settings = parsed(unsorted);

    // Assert: the document's list order is contractual, so it is decided here rather
    // than inherited from whatever order the server answered in.
    let names: Vec<&str> = settings
        .iter()
        .map(|setting| setting.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["DateStyle", "max_connections", "shared_buffers"]
    );
}

#[test]
fn parse_refuses_a_row_with_the_wrong_number_of_columns() {
    // Act
    let refused = PsqlSettings::parse("max_connections,100,,configuration file\n");

    // Assert: a short row means the query and this parser disagree about the columns,
    // and guessing which one is missing would put a value under the wrong name.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_output_with_no_settings_in_it() {
    // Act
    let refused = PsqlSettings::parse("\n");

    // Assert: every cluster has hundreds of settings, so nothing at all is a failed
    // read rather than a cluster with no configuration.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_two_rows_for_one_setting() {
    // Arrange
    let contradiction = "\
max_connections,100,,configuration file,postmaster,f
max_connections,200,,configuration file,postmaster,f
";

    // Act
    let refused = PsqlSettings::parse(contradiction);

    // Assert
    assert!(refused.is_err());
}
