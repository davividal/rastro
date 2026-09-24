//! Reading a cluster's client-authentication rules, without a cluster to read them from.
//!
//! `pg_hba_file_rules` is who may connect as whom, from where, and how, which `pg_settings`
//! does not carry. The view gained `rule_number` and `file_name` in PostgreSQL 16, so the
//! parser reads two shapes: eleven columns on 16 and later, nine on 15.

use rastro::collectors::postgresql::{ClusterId, HbaRule, PsqlHbaRules};

/// The eleven columns PostgreSQL 16 and later print.
///
/// Rule 2 before rule 1: the server prints rules in file order, not rule-number order (the
/// two only coincide when nothing has ever reordered the files), so listing them in this
/// order is what makes `ClusterHbaRules`'s sort by `rule_number` observable rather than
/// incidentally already-sorted input.
const RULES_V16: &str = "\
2,/etc/postgresql/17/main/pg_hba.conf,92,host,{all},{all},127.0.0.1/32,,scram-sha-256,{},
1,/etc/postgresql/17/main/pg_hba.conf,90,local,{all},{postgres},,,peer,{},
";

/// The nine columns PostgreSQL 15 prints: no rule_number, no file_name.
///
/// Line 92 before line 90: PostgreSQL 15 has no `rule_number` to sort by, so the fallback
/// key is `line_number`, and listing the higher line first is what makes that fallback sort
/// observable.
const RULES_V15: &str = "\
92,host,{all},{all},127.0.0.1/32,,scram-sha-256,{},
90,local,{all},{postgres},,,peer,{},
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
fn parse_sorts_by_rule_number_rather_than_the_order_the_server_printed_them() {
    // Act: `RULES_V16` lists rule 2 before rule 1, since precedence order and file order
    // are not the same thing.
    let rules = parsed(RULES_V16);

    // Assert
    assert_eq!(
        rules
            .iter()
            .map(|rule| rule.rule_number)
            .collect::<Vec<Option<i64>>>(),
        [Some(1), Some(2)]
    );
}

#[test]
fn parse_sorts_by_line_number_when_no_rule_number_exists() {
    // Act: `RULES_V15` has no rule_number column at all, and lists line 92 before line 90.
    let rules = parsed(RULES_V15);

    // Assert
    assert_eq!(
        rules
            .iter()
            .map(|rule| rule.line_number)
            .collect::<Vec<Option<i64>>>(),
        [Some(90), Some(92)]
    );
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
