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

/// The arguments a file holds, in order.
pub fn arguments_in(text: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current: Option<String> = None;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '#' if current.is_none() => {
                for skipped in characters.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
            }
            '\'' | '"' => {
                let closing = character;
                let argument = current.get_or_insert_with(String::new);
                while let Some(quoted) = characters.next() {
                    match quoted {
                        quote if quote == closing => break,
                        '\\' => match characters.next() {
                            Some('\n') => {
                                while characters
                                    .next_if(|next| *next == ' ' || *next == '\t')
                                    .is_some()
                                {}
                            }
                            Some('\r') => {
                                characters.next_if_eq(&'\n');
                                while characters
                                    .next_if(|next| *next == ' ' || *next == '\t')
                                    .is_some()
                                {}
                            }
                            Some('t') => argument.push('\t'),
                            Some('n') => argument.push('\n'),
                            Some('r') => argument.push('\r'),
                            Some('f') => argument.push('\u{c}'),
                            Some(escaped) => argument.push(escaped),
                            None => {}
                        },
                        other => argument.push(other),
                    }
                }
            }
            whitespace if whitespace.is_whitespace() => {
                if let Some(argument) = current.take() {
                    arguments.push(argument);
                }
            }
            other => current.get_or_insert_with(String::new).push(other),
        }
    }

    if let Some(argument) = current {
        arguments.push(argument);
    }
    arguments
}
