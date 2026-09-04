//! The nginx configuration grammar.
//!
//! Every escape asserted here was measured against nginx 1.30 rather than remembered.
//! A quoted `access_log` path becomes a file when nginx tests a config, so the name that
//! appeared on disk is exactly what nginx's own parser made of the token: `\t`, `\n`,
//! `\"` and `\\` are the characters they name, an unrecognised `\q` keeps its backslash,
//! and a `#` that is not at the start of a token is an ordinary character.

use rastro::collectors::nginx::conf_syntax;
use rastro::collectors::nginx::model::Directive;

fn directives(text: &str) -> Vec<Directive> {
    conf_syntax::parse(text).expect("this configuration is well formed")
}

fn only(text: &str) -> Directive {
    let mut parsed = directives(text);
    assert_eq!(parsed.len(), 1, "expected one directive from {text:?}");
    parsed.remove(0)
}

fn arguments_of(directive: &Directive) -> Vec<&str> {
    directive
        .arguments
        .iter()
        .map(|argument| argument.as_str())
        .collect()
}

fn refusal(text: &str) -> String {
    conf_syntax::parse(text)
        .expect_err("this configuration is not well formed")
        .to_string()
}

#[test]
fn a_directive_is_a_name_and_its_arguments() {
    // Act
    let directive = only("worker_processes auto;");

    // Assert
    assert_eq!(directive.name.as_str(), "worker_processes");
    assert_eq!(arguments_of(&directive), ["auto"]);
    assert!(directive.block.is_none());
}

#[test]
fn a_block_holds_the_directives_inside_it() {
    // Act
    let directive = only("server {\n    listen 80;\n    server_name example.org;\n}");

    // Assert
    assert_eq!(directive.name.as_str(), "server");
    let inside = directive.block.expect("a server directive opens a block");
    assert_eq!(
        inside
            .iter()
            .map(|child| child.name.as_str())
            .collect::<Vec<&str>>(),
        ["listen", "server_name"]
    );
}

#[test]
fn a_block_may_be_empty() {
    // Act
    let directive = only("events { }");

    // Assert
    assert_eq!(directive.block, Some(Vec::new()));
}

#[test]
fn a_comment_runs_to_the_end_of_the_line() {
    // Act
    let parsed = directives("# a whole line\nuser nginx; # and a trailing one\n");

    // Assert
    assert_eq!(parsed.len(), 1);
    assert_eq!(arguments_of(&parsed[0]), ["nginx"]);
}

#[test]
fn a_hash_inside_a_token_is_not_a_comment() {
    // Arrange: measured, not assumed. nginx starts a comment only where a token starts.
    // Act
    let directive = only("access_log /var/log/nginx/a#b.log;");

    // Assert
    assert_eq!(arguments_of(&directive), ["/var/log/nginx/a#b.log"]);
}

#[test]
fn a_quoted_argument_loses_its_quotes() {
    // Arrange: quoting is spelling, not state. Requoting a value must not read as a change.
    // Act
    let directive = only(r#"log_format main "a b";"#);

    // Assert
    assert_eq!(arguments_of(&directive), ["main", "a b"]);
}

#[test]
fn an_escape_inside_quotes_is_the_character_it_names() {
    // Act
    let directive = only(r#"log_format main "a\tb\nc\"d\\e";"#);

    // Assert
    assert_eq!(arguments_of(&directive), ["main", "a\tb\nc\"d\\e"]);
}

#[test]
fn an_unknown_escape_keeps_its_backslash() {
    // Arrange: nginx passes an unrecognised escape through whole, so neither character
    // may be dropped here.
    // Act
    let directive = only(r#"root "/srv/g\qh";"#);

    // Assert
    assert_eq!(arguments_of(&directive), [r"/srv/g\qh"]);
}

#[test]
fn single_quotes_escape_the_same_way() {
    // Act
    let directive = only("log_format main 'a\\tb';");

    // Assert
    assert_eq!(arguments_of(&directive), ["main", "a\tb"]);
}

#[test]
fn an_empty_argument_survives() {
    // Arrange: `""` is a real value to nginx, and a directive that lost it would read as
    // a different directive.
    // Act
    let directive = only(r#"add_header X-Empty "";"#);

    // Assert
    assert_eq!(arguments_of(&directive), ["X-Empty", ""]);
}

#[test]
fn a_quote_that_is_not_at_the_start_of_a_token_is_an_ordinary_character() {
    // Act
    let directive = only(r#"server_name a"b;"#);

    // Assert
    assert_eq!(arguments_of(&directive), [r#"a"b"#]);
}

#[test]
fn an_unclosed_block_names_where_it_opened() {
    // Act
    let refused = refusal("http {\n    server {\n        listen 80;\n    }\n");

    // Assert
    assert!(refused.contains("line 1"), "{refused}");
}

#[test]
fn an_unmatched_close_is_a_failure() {
    // Act
    let refused = refusal("listen 80;\n}\n");

    // Assert
    assert!(refused.contains("line 2"), "{refused}");
}

#[test]
fn an_unterminated_quote_is_a_failure() {
    // Act
    let refused = refusal("log_format main \"never closed;\n");

    // Assert
    assert!(refused.contains("line 1"), "{refused}");
}

#[test]
fn a_directive_that_is_never_terminated_is_a_failure() {
    // Act
    let refused = refusal("worker_processes auto\n");

    // Assert
    assert!(refused.contains("worker_processes"), "{refused}");
}

#[test]
fn an_escape_outside_quotes_is_the_character_it_names() {
    // Arrange: measured. nginx spends a backslash in a bare token the same way it spends
    // one inside quotes.
    // Act
    let directive = only("access_log /var/log/nginx/a\\tb.log;");

    // Assert
    assert_eq!(arguments_of(&directive), ["/var/log/nginx/a\tb.log"]);
}

#[test]
fn a_backslash_hides_a_delimiter_from_the_grammar() {
    // Arrange: measured. `\;` inside a bare token terminates nothing, so a grammar that
    // ended the token there would read two directives where nginx reads one.
    // Act
    let directive = only(r"root /srv/a\;b;");

    // Assert
    assert_eq!(arguments_of(&directive), [r"/srv/a\;b"]);
}

#[test]
fn a_semicolon_that_terminates_nothing_is_a_failure() {
    // Act
    let refused = refusal("http { }\n;\n");

    // Assert
    assert!(refused.contains("line 2"), "{refused}");
}

#[test]
fn a_block_that_no_directive_names_is_a_failure() {
    // Act
    let refused = refusal("http {\n    { listen 80; }\n}\n");

    // Assert
    assert!(refused.contains("line 2"), "{refused}");
}

#[test]
fn a_trailing_backslash_ends_the_file_rather_than_the_reader() {
    // Arrange: a backslash with nothing after it has nothing to escape. The failure belongs
    // to whoever was waiting for a `;`, not to the escape itself, and neither may loop.
    // Act
    let refused = refusal("root /srv/a\\");

    // Assert
    assert!(refused.contains("root"), "{refused}");
}
