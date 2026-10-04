//! A `java` argument file, read as the `java` launcher reads it.
//!
//! Measured on the JDK bundled with Elasticsearch 8.15.3, since the syntax is the launcher's and
//! a reading that differs from it reads another argv than the JVM ran with:
//!
//! - whitespace separates arguments, and `'…'` or `"…"` groups them, the quotes removed;
//! - `#` where an argument would start comments to the end of the line;
//! - outside quotes a backslash is itself; inside them `\t`, `\n`, `\r` and `\f` are those
//!   characters, any other escaped character is that character, and a backslash before a line
//!   break continues onto the next line with its leading spaces dropped;
//! - an `@name` inside a file is an argument, not another file: files do not nest.

use std::iter::Peekable;
use std::str::Chars;

/// The arguments a file holds, in order.
pub fn arguments_in(text: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current: Option<String> = None;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '#' if current.is_none() => skip_comment(&mut characters),
            '\'' | '"' => {
                read_quoted(
                    &mut characters,
                    character,
                    current.get_or_insert_with(String::new),
                );
            }
            whitespace if whitespace.is_whitespace() => arguments.extend(current.take()),
            other => current.get_or_insert_with(String::new).push(other),
        }
    }

    arguments.extend(current);
    arguments
}

/// Past a comment, to the end of its line.
fn skip_comment(characters: &mut Peekable<Chars<'_>>) {
    for skipped in characters.by_ref() {
        if skipped == '\n' {
            break;
        }
    }
}

/// A quoted section into `argument`, up to the `closing` quote, its escapes read.
fn read_quoted(characters: &mut Peekable<Chars<'_>>, closing: char, argument: &mut String) {
    while let Some(quoted) = characters.next() {
        match quoted {
            quote if quote == closing => return,
            '\\' => read_escape(characters, argument),
            other => argument.push(other),
        }
    }
}

/// What follows a backslash inside quotes: a line continuation, or one escaped character.
fn read_escape(characters: &mut Peekable<Chars<'_>>, argument: &mut String) {
    match characters.next() {
        Some('\n') => skip_indentation(characters),
        Some('\r') => {
            characters.next_if_eq(&'\n');
            skip_indentation(characters);
        }
        Some('t') => argument.push('\t'),
        Some('n') => argument.push('\n'),
        Some('r') => argument.push('\r'),
        Some('f') => argument.push('\u{c}'),
        Some(escaped) => argument.push(escaped),
        None => {}
    }
}

/// Past the leading spaces of a continued line, which the launcher drops.
fn skip_indentation(characters: &mut Peekable<Chars<'_>>) {
    while characters
        .next_if(|next| *next == ' ' || *next == '\t')
        .is_some()
    {}
}
