//! The credentials an operator hands a run: one `NAME=value` per line, from a file or stdin.
//!
//! Never in the config and never on the argv, which the `invocation` facet records and `ps`
//! shows. A value reaches the collector that asked for it and nothing else: the document names
//! which credentials were given, never what they hold.

use rastro::credentials::Credentials;

#[test]
fn parse_reads_one_name_and_value_per_line() {
    // Act
    let credentials = Credentials::parse("ELASTICSEARCH_API_KEY=abc123\n").expect("credentials");

    // Assert
    assert_eq!(credentials.get("ELASTICSEARCH_API_KEY"), Some("abc123"));
}

#[test]
fn parse_skips_blank_lines_and_comments() {
    // Act
    let credentials =
        Credentials::parse("# for the search box\n\nELASTICSEARCH_API_KEY=abc\n   \n")
            .expect("credentials");

    // Assert
    assert_eq!(credentials.names(), ["ELASTICSEARCH_API_KEY"]);
}

#[test]
fn parse_takes_a_quoted_value_without_its_quotes() {
    // Arrange: how a secret manager's template or a shell `.env` writes a value.
    let text = "ELASTICSEARCH_USERNAME=\"elastic\"\nELASTICSEARCH_PASSWORD='p w'\n";

    // Act
    let credentials = Credentials::parse(text).expect("credentials");

    // Assert
    assert_eq!(credentials.get("ELASTICSEARCH_USERNAME"), Some("elastic"));
    assert_eq!(credentials.get("ELASTICSEARCH_PASSWORD"), Some("p w"));
}

#[test]
fn parse_keeps_an_equals_sign_inside_the_value() {
    // Arrange: an encoded API key ends in `=` padding.
    let credentials = Credentials::parse("ELASTICSEARCH_API_KEY=a2V5Og==\n").expect("credentials");

    // Act & Assert
    assert_eq!(credentials.get("ELASTICSEARCH_API_KEY"), Some("a2V5Og=="));
}

#[test]
fn parse_takes_a_shell_export_as_the_name_it_exports() {
    // Act
    let credentials =
        Credentials::parse("export ELASTICSEARCH_API_KEY=abc\n").expect("credentials");

    // Assert
    assert_eq!(credentials.get("ELASTICSEARCH_API_KEY"), Some("abc"));
}

#[test]
fn parse_refuses_a_line_that_is_not_a_name_and_a_value_naming_the_line_not_its_content() {
    // Arrange: the line may be the secret itself, pasted without its name.
    let text = "ELASTICSEARCH_USERNAME=elastic\nhunter2\n";

    // Act
    let refusal = Credentials::parse(text).expect_err("a stray line");

    // Assert
    assert!(refusal.contains("line 2"), "{refusal}");
    assert!(!refusal.contains("hunter2"), "{refusal}");
}

#[test]
fn parse_refuses_a_name_given_twice() {
    // Act
    let refusal = Credentials::parse("ELASTICSEARCH_API_KEY=a\nELASTICSEARCH_API_KEY=b\n")
        .expect_err("a repeated name");

    // Assert
    assert!(refusal.contains("ELASTICSEARCH_API_KEY"), "{refusal}");
    assert!(refusal.contains("line 2"), "{refusal}");
}

#[test]
fn names_lists_what_was_given_and_never_a_value() {
    // Arrange
    let credentials =
        Credentials::parse("ELASTICSEARCH_PASSWORD=s3cret\nELASTICSEARCH_USERNAME=u\n")
            .expect("credentials");

    // Act
    let names = credentials.names();

    // Assert
    assert_eq!(names, ["ELASTICSEARCH_PASSWORD", "ELASTICSEARCH_USERNAME"]);
    assert!(!format!("{credentials:?}").contains("s3cret"));
}
