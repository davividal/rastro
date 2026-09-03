//! nginx's configuration grammar, as nginx's own parser reads it.
//!
//! A small language: tokens separated by whitespace, `;` to terminate a directive, `{` and
//! `}` to nest one, `#` to comment out the rest of a line, and either quote character to
//! hold a token that contains any of those.
//!
//! **Measured against nginx 1.30 rather than remembered.** Testing a configuration opens
//! every log file it names and creates the missing ones, so a quoted `access_log` path
//! becomes a filename on disk that shows exactly what nginx made of the token. That is how
//! the escape rules below were established, and it is also why the collector never runs the
//! test on a host it is describing.
//!
//! What this layer does *not* do is as deliberate as what it does. It applies no
//! inheritance, resolves no variable, and knows no directive by name: `root` in an `http`
//! block reaching the servers under it is a fact about nginx's semantics, and belongs to
//! whatever models those, not to the grammar.

use rastro_collector::CollectionError;

use crate::collectors::nginx::model::Directive;
use crate::collectors::nginx::value_objects::{DirectiveArgument, DirectiveName};

/// The characters that end a bare token wherever they appear in one.
const DELIMITERS: [char; 3] = [';', '{', '}'];

/// Turns a configuration file's text into the directives it holds.
///
/// Nesting is kept, order is kept, and nothing is dropped but comments and whitespace.
/// The result is one file's own directives: an `include` is still a directive here, because
/// resolving it means reading the filesystem and this layer reads only text.
pub fn parse(text: &str) -> Result<Vec<Directive>, CollectionError> {
    read_directives(&mut Tokens::new(text), None)
}

/// What nginx makes of a backslash inside a quoted token.
///
/// Six characters are the escape and everything else keeps both characters, which is why
/// this answers `None` rather than listing what nginx passes through. `\q` measured as
/// `\q`, so dropping the backslash for an escape nobody recognised would put a value in the
/// document that was never in the file.
fn escaped(character: char) -> Option<char> {
    match character {
        '"' => Some('"'),
        '\'' => Some('\''),
        '\\' => Some('\\'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        _ => None,
    }
}

/// The directives of one block, up to its `}`, or of the whole file when `opened_at` is
/// `None`.
///
/// The caller's `opened_at` is the line the block's `{` was on, and it is what tells the
/// two failures apart: a `}` with no block open is one mistake, a block that runs off the
/// end of the file is the other, and each names the line a reader needs.
fn read_directives(
    tokens: &mut Tokens,
    opened_at: Option<usize>,
) -> Result<Vec<Directive>, CollectionError> {
    let mut directives = Vec::new();

    loop {
        let Some(located) = tokens.next_token()? else {
            return match opened_at {
                Some(line) => Err(CollectionError::new(format!(
                    "a block opened on line {line} was never closed"
                ))),
                None => Ok(directives),
            };
        };

        match located.token {
            Token::BlockClose => {
                return match opened_at {
                    Some(_) => Ok(directives),
                    None => Err(CollectionError::new(format!(
                        "the `}}` on line {} closes a block that was never opened",
                        located.line
                    ))),
                };
            }
            Token::Terminator => {
                return Err(CollectionError::new(format!(
                    "the `;` on line {} terminates no directive",
                    located.line
                )));
            }
            Token::BlockOpen => {
                return Err(CollectionError::new(format!(
                    "the `{{` on line {} opens a block that no directive names",
                    located.line
                )));
            }
            Token::Word(name) => directives.push(read_directive(tokens, &name, located.line)?),
        }
    }
}

/// One directive, from the token that named it to its `;` or its block.
fn read_directive(
    tokens: &mut Tokens,
    name: &str,
    line: usize,
) -> Result<Directive, CollectionError> {
    let name = DirectiveName::new(name)?;
    let mut arguments = Vec::new();

    loop {
        let Some(located) = tokens.next_token()? else {
            return Err(CollectionError::new(format!(
                "{} on line {line} reaches the end of the file with no `;` and no block",
                name.as_str()
            )));
        };

        match located.token {
            Token::Word(argument) => arguments.push(DirectiveArgument::new(argument)),
            Token::Terminator => {
                return Ok(Directive {
                    name,
                    arguments,
                    block: None,
                });
            }
            Token::BlockOpen => {
                let block = read_directives(tokens, Some(located.line))?;
                return Ok(Directive {
                    name,
                    arguments,
                    block: Some(block),
                });
            }
            Token::BlockClose => {
                return Err(CollectionError::new(format!(
                    "{} on line {line} is closed by the `}}` on line {} rather than terminated",
                    name.as_str(),
                    located.line
                )));
            }
        }
    }
}

/// What the grammar is made of, once the whitespace and the comments are gone.
#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Terminator,
    BlockOpen,
    BlockClose,
}

/// A token and the line it started on, so a refusal can say where to look.
#[derive(Debug)]
struct LocatedToken {
    token: Token,
    line: usize,
}

/// The text, and how far into it the reader has got.
///
/// Characters rather than bytes, because a refusal quotes the token it choked on and a
/// configuration is UTF-8 with server names that are not ASCII.
struct Tokens {
    characters: Vec<char>,
    position: usize,
    line: usize,
}

impl Tokens {
    fn new(text: &str) -> Self {
        Self {
            characters: text.chars().collect(),
            position: 0,
            line: 1,
        }
    }

    fn next_token(&mut self) -> Result<Option<LocatedToken>, CollectionError> {
        self.skip_blanks();

        let Some(character) = self.peek() else {
            return Ok(None);
        };
        let line = self.line;

        let token = match character {
            ';' => self.punctuation(Token::Terminator),
            '{' => self.punctuation(Token::BlockOpen),
            '}' => self.punctuation(Token::BlockClose),
            '"' | '\'' => Token::Word(self.quoted(character)?),
            _ => Token::Word(self.bare()),
        };

        Ok(Some(LocatedToken { token, line }))
    }

    /// Whitespace and comments, which a token is never made of.
    ///
    /// A `#` is a comment only where a token begins. `a#b` is one token, measured, so the
    /// two are skipped in one loop rather than the comment being stripped up front.
    fn skip_blanks(&mut self) {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.take();
            }

            if self.peek() != Some('#') {
                return;
            }

            while self.peek().is_some_and(|character| character != '\n') {
                self.take();
            }
        }
    }

    fn punctuation(&mut self, token: Token) -> Token {
        self.take();
        token
    }

    /// A token held between quotes, with its escapes spent and its quotes discarded.
    fn quoted(&mut self, quote: char) -> Result<String, CollectionError> {
        let opened_at = self.line;
        self.take();
        let mut value = String::new();

        loop {
            let Some(character) = self.take() else {
                return Err(CollectionError::new(format!(
                    "the {quote} opened on line {opened_at} was never closed"
                )));
            };

            match character {
                _ if character == quote => return Ok(value),
                '\\' => self.escape(&mut value),
                _ => value.push(character),
            }
        }
    }

    /// A token with no quotes, which runs to whitespace or to a delimiter.
    ///
    /// A quote inside one is an ordinary character: `server_name a"b;` is the single token
    /// `a"b`, measured, because nginx opens a quoted token only where a token begins.
    ///
    /// **Escapes work out here too, and a backslash is spent before a delimiter is looked
    /// for.** Measured: `a\tb` outside quotes holds a tab, and `a\;b` is one token whose
    /// semicolon terminates nothing. A grammar that ended the token at that `;` would report
    /// two directives where nginx reads one.
    fn bare(&mut self) -> String {
        let mut value = String::new();

        while let Some(character) = self.peek() {
            if character == '\\' {
                self.take();
                self.escape(&mut value);
                continue;
            }

            if character.is_whitespace() || DELIMITERS.contains(&character) {
                break;
            }

            value.push(character);
            self.take();
        }

        value
    }

    /// Spends the character after a backslash, which the caller has already taken.
    ///
    /// A backslash at the very end of the file has nothing to stand in front of and stays as
    /// it is, which leaves the failure to whoever was waiting for a quote or a `;`.
    fn escape(&mut self, into: &mut String) {
        let Some(character) = self.take() else {
            into.push('\\');
            return;
        };

        match escaped(character) {
            Some(plain) => into.push(plain),
            None => {
                into.push('\\');
                into.push(character);
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn take(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += 1;

        if character == '\n' {
            self.line += 1;
        }

        Some(character)
    }
}
