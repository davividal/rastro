//! Reading a cluster's client-authentication rules, without a cluster to read them from.
//!
//! `pg_hba_file_rules` is who may connect as whom, from where, and how, which `pg_settings`
//! does not carry. The view gained `rule_number` and `file_name` in PostgreSQL 16, so the
//! parser reads two shapes: eleven columns on 16 and later, nine on 15.

use rastro::collectors::postgresql::{ClusterId, HbaRule, PsqlHbaRules};

/// The eleven columns PostgreSQL 16 and later print.
const RULES_V16: &str = "\
1,/etc/postgresql/17/main/pg_hba.conf,90,local,{all},{postgres},,,peer,{},
2,/etc/postgresql/17/main/pg_hba.conf,92,host,{all},{all},127.0.0.1/32,,scram-sha-256,{},
";

/// The nine columns PostgreSQL 15 prints: no rule_number, no file_name.
const RULES_V15: &str = "\
90,local,{all},{postgres},,,peer,{},
92,host,{all},{all},127.0.0.1/32,,scram-sha-256,{},
";

fn parsed(csv: &str) -> Vec<HbaRule> {
    PsqlHbaRules::parse(csv)
        .expect("this output is well formed")
        .rules()
        .to_vec()
}

#[test]
fn parse_reads_the_eleven_column_shape() {
    // Act
    let rules = parsed(RULES_V16);

    // Assert: the PostgreSQL 16 columns are present, and the local rule's null address is an
    // absent value rather than an empty string.
    let local = &rules[0];
    assert_eq!(local.rule_number, Some(1));
    assert_eq!(
        local.file_name.as_deref(),
        Some("/etc/postgresql/17/main/pg_hba.conf")
    );
    assert_eq!(local.connection_type.as_deref(), Some("local"));
    assert_eq!(local.auth_method.as_deref(), Some("peer"));
    assert_eq!(local.address, None);
}

#[test]
fn parse_reads_the_nine_column_shape() {
    // Act
    let rules = parsed(RULES_V15);

    // Assert: on PostgreSQL 15 the rule number and file name are absent, and the rest read
    // the same, so a 15 cluster is not lost for want of two columns it never had.
    let host = &rules[1];
    assert_eq!(host.rule_number, None);
    assert_eq!(host.file_name, None);
    assert_eq!(host.connection_type.as_deref(), Some("host"));
    assert_eq!(host.address.as_deref(), Some("127.0.0.1/32"));
    assert_eq!(host.auth_method.as_deref(), Some("scram-sha-256"));
}

#[test]
fn parse_keeps_an_array_column_as_the_server_rendered_it() {
    // Act
    let rules = parsed(RULES_V16);

    // Assert: database and user_name are text arrays, kept as `{...}` for a run-to-run diff
    // rather than split into something to compute on.
    assert_eq!(rules[0].databases.as_deref(), Some("{all}"));
    assert_eq!(rules[0].users.as_deref(), Some("{postgres}"));
}

#[test]
fn parse_refuses_a_row_that_is_neither_shape() {
    // Act & Assert: a row of some other width means the query and the parser disagree, which
    // is a failure rather than a rule read under the wrong columns.
    assert!(PsqlHbaRules::parse("local,{all},peer\n").is_err());
}

#[test]
fn a_cluster_id_reports_its_major_version() {
    // Act & Assert: the major version decides which shape to ask for, and it is the integer
    // before the first dot, so both the modern and the old spelling read.
    assert_eq!(
        ClusterId::new("17", "main").expect("legal").major_version(),
        Some(17)
    );
    assert_eq!(
        ClusterId::new("9.6", "main")
            .expect("legal")
            .major_version(),
        Some(9)
    );
}
