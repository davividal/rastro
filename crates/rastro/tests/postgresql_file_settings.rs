//! Reading a cluster's configuration-file lines, without a cluster to read them from.
//!
//! `pg_file_settings` is the missing half of the collector's thesis: it re-parses the files,
//! so it sees a value edited without a reload and a line that will not apply, neither of
//! which `pg_settings` can show. The fixtures carry a plain line, a credential-bearing one,
//! and a drop-in that fails to apply.

mod support;

use rastro::collectors::postgresql::{FileSetting, PsqlFileSettings, SettingName};
use rastro_fingerprint::{Observation, Sensitivity};
use support::observation::{field, is_null, text};

/// The seven columns the collector's query asks for, in order.
const FILE_SETTINGS: &str = "\
1,/etc/postgresql/17/main/postgresql.conf,110,max_connections,100,t,
2,/etc/postgresql/17/main/postgresql.conf,112,archive_command,rsync %p backup:%f,t,
3,/etc/postgresql/17/main/conf.d/10-bad.conf,4,shared_buffers,notasize,f,invalid value for parameter shared_buffers
";

fn parsed(csv: &str) -> Vec<FileSetting> {
    PsqlFileSettings::parse(csv)
        .expect("this output is well formed")
        .settings()
        .to_vec()
}

fn named<'a>(settings: &'a [FileSetting], name: &str) -> &'a FileSetting {
    let wanted = SettingName::new(name).expect("a legal name");
    settings
        .iter()
        .find(|setting| setting.name.as_ref() == Some(&wanted))
        .expect("the fixture has this line")
}

#[test]
fn parse_reads_a_line_and_where_it_came_from() {
    // Act
    let settings = parsed(FILE_SETTINGS);

    // Assert
    let max_connections = named(&settings, "max_connections");
    assert_eq!(max_connections.seqno, 1);
    assert_eq!(
        max_connections.sourcefile.as_deref(),
        Some("/etc/postgresql/17/main/postgresql.conf")
    );
    assert_eq!(max_connections.sourceline, Some(110));
    assert!(max_connections.applied);
    assert_eq!(max_connections.error, None);
}

#[test]
fn parse_reads_a_line_that_will_not_apply_with_its_error() {
    // Act
    let settings = parsed(FILE_SETTINGS);

    // Assert: applied = false with an error is the typo'd drop-in that reads as fine in the
    // file, which a file comparison would pass straight over.
    let broken = named(&settings, "shared_buffers");
    assert!(!broken.applied);
    assert_eq!(
        broken.error.as_deref(),
        Some("invalid value for parameter shared_buffers")
    );
}

#[test]
fn parse_orders_lines_by_the_sequence_the_server_read_them() {
    // Arrange: given out of order, because precedence follows the read order and the model
    // owns that order rather than the query.
    let shuffled = "\
3,/f.conf,1,work_mem,4MB,t,
1,/a.conf,1,max_connections,100,t,
2,/b.conf,1,shared_buffers,128MB,t,
";

    // Act
    let seqnos: Vec<i64> = parsed(shuffled)
        .iter()
        .map(|setting| setting.seqno)
        .collect();

    // Assert
    assert_eq!(seqnos, vec![1, 2, 3]);
}

#[test]
fn a_credential_bearing_file_line_is_redacted() {
    // Act: pg_file_settings carries the raw, uncanonicalised file line, so a credential in
    // archive_command sits here verbatim and must be withheld.
    let observation = Observation::from(named(&parsed(FILE_SETTINGS), "archive_command"));

    // Assert: the content is withheld, not just annotated, so the raw archive_command never
    // reaches the document.
    let value = field(&observation, "value");
    assert_eq!(text(&value), "[redacted]");
    assert_eq!(value.sensitivity(), Sensitivity::Sensitive);
}

#[test]
fn new_refuses_one_seqno_reported_twice() {
    // Act & Assert: seqno is a sequence number, so a repeat means two reads were spliced and
    // precedence can no longer be told.
    let repeated = "\
1,/a.conf,1,max_connections,100,t,
1,/b.conf,1,shared_buffers,128MB,t,
";
    assert!(PsqlFileSettings::parse(repeated).is_err());
}

#[test]
fn parse_reads_an_unparseable_line_with_no_name_and_its_error() {
    // Arrange: a line PostgreSQL could not parse leaves `name` and `setting` null while the
    // `error` column explains it. This is exactly the malformed line the catalogue exists to
    // surface, so it must be recorded, not refused.
    let unparseable = "\
1,/etc/postgresql/17/main/postgresql.conf,7,,,f,syntax error
";

    // Act
    let settings = parsed(unparseable);

    // Assert
    assert_eq!(settings.len(), 1);
    let line = &settings[0];
    assert_eq!(line.name, None);
    assert!(!line.applied);
    assert_eq!(line.error.as_deref(), Some("syntax error"));
}

#[test]
fn an_unparseable_line_renders_a_null_name() {
    // Act
    let unparseable = "1,/etc/postgresql/17/main/postgresql.conf,7,,,f,syntax error\n";
    let observation = Observation::from(&parsed(unparseable)[0]);

    // Assert: the missing name reaches the document as null rather than an empty string.
    assert!(is_null(&field(&observation, "name")));
}
