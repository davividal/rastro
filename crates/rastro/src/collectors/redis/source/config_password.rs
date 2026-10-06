//! The directives rastro reads out of a redis configuration file: the ones that decide the
//! password.
//!
//! **Not a reading of the configuration.** What a server runs with comes from the server, and
//! this file is opened only because a server that wants a password has no other way to be
//! asked. Three directives are looked for through the `include`s the server followed:
//! `requirepass`, the `user default` line that outranks it, and `aclfile`, which outranks both.
//! Nothing found here reaches the document.
//!
//! **Tokenised the way redis tokenises it**, `sdssplitargs`, because a password is exactly the
//! value most likely to hold a quote or a space, and reading it differently from the server means
//! sending it a wrong password, which is an entry in its `ACL LOG`.

use std::fmt;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use crate::collectors::file_glob;
use crate::collectors::inside_root::{names_inside, read_inside};

const REQUIREPASS: &str = "requirepass";
const INCLUDE: &str = "include";
const USER: &str = "user";
const ACLFILE: &str = "aclfile";

/// The account `requirepass` sets, and the only one whose password rastro ever needs.
const DEFAULT_USER: &str = "default";

/// The most files one configuration is read to, includes and their includes together.
///
/// A budget of work rather than a depth: depth alone bounds nesting, and fifteen files each
/// including the next three times took 35 s, measured. It also stops a file that includes itself,
/// directly or round a loop, without a set of open files. A real configuration names a handful.
const MOST_FILES: usize = 64;

/// What a configuration file says about the default account's password.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct PasswordDirectives {
    /// The last `requirepass`, where it sets one; `requirepass ""` clears an earlier one.
    pub requirepass: Option<String>,

    /// The rules of the `user default` line, in order, where there is one.
    pub default_user: Option<Vec<String>>,

    /// The ACL file, where one is named; the server then ignores `requirepass` entirely.
    pub acl_file: Option<PathBuf>,
}

/// Says which directives were found and never what they hold, so a `{:?}` in some later message
/// cannot carry a password.
impl fmt::Debug for PasswordDirectives {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordDirectives")
            .field(
                "requirepass",
                &self.requirepass.as_ref().map(|_| "<withheld>"),
            )
            .field("default_user", &self.default_user.as_ref().map(Vec::len))
            .field("acl_file", &self.acl_file)
            .finish()
    }
}

/// The password directives the file leaves the server with, every path resolved inside `root`.
///
/// **Inside the server's root, not the host's**, measured: the package's unit gives every server
/// a mount namespace of its own (`PrivateTmp=yes`, `ReadOnlyDirectories=/`), and a container's or a
/// `RootDirectory=` unit's paths are its own, so `/proc/<pid>/root` is where its paths mean what
/// they meant to the server.
///
/// **The last `requirepass` wins**, the file read top to bottom with each include read in place,
/// because that is the order the server applies them in. **A second `user default` is refused**:
/// measured, the server will not start on such a file, so it is not the file of a running one.
pub fn password_directives_in(
    root: &Path,
    file: &Path,
) -> Result<PasswordDirectives, CollectionError> {
    let mut directives = PasswordDirectives::default();
    read_into(root, file, &mut 0, &mut directives)?;
    directives.requirepass = directives
        .requirepass
        .filter(|password: &String| !password.is_empty());

    Ok(directives)
}

/// The rules of the `default` account in an ACL file, or nothing where it declares none.
///
/// Nothing is an answer, not a failure: measured, a server whose ACL file declares no `default`
/// leaves that account without a password, whatever `requirepass` says.
pub fn default_user_in_acl_file(
    root: &Path,
    file: &Path,
) -> Result<Option<Vec<String>>, CollectionError> {
    let text = read_bounded(root, file)?;

    let mut default_user = None;
    for line in text.lines() {
        let Some(words) = words_of(file, line)? else {
            continue;
        };
        if let Some(rules) = default_user_rules(&words)
            && default_user.replace(rules).is_some()
        {
            return Err(declared_twice(file));
        }
    }

    Ok(default_user)
}

fn read_into(
    root: &Path,
    file: &Path,
    files_read: &mut usize,
    directives: &mut PasswordDirectives,
) -> Result<(), CollectionError> {
    *files_read += 1;
    if *files_read > MOST_FILES {
        return Err(CollectionError::new(format!(
            "{} is past the {MOST_FILES} files a configuration is read to, which is a file \
             including itself or an include tree wider than any real one",
            file.display()
        )));
    }

    let text = read_bounded(root, file)?;

    for line in text.lines() {
        let Some(words) = words_of(file, line)? else {
            continue;
        };

        if let Some(rules) = default_user_rules(&words) {
            if directives.default_user.replace(rules).is_some() {
                return Err(declared_twice(file));
            }
            continue;
        }

        match (words[0].to_ascii_lowercase().as_str(), words.len()) {
            (REQUIREPASS, 2) => directives.requirepass = Some(words[1].clone()),
            (ACLFILE, 2) => directives.acl_file = Some(PathBuf::from(&words[1])),
            (INCLUDE, 2) => {
                for included in included_files(root, file, &words[1])? {
                    read_into(root, &included, files_read, directives)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// A file's text, read bounded and only where it is a regular file.
///
/// The file is the redis account's, Debian's `redis.conf` among them, so what is at the path is
/// that account's choice: a FIFO there blocked the run and `/dev/zero` grew without end, the
/// finding the elasticsearch collector met first.
fn read_bounded(root: &Path, file: &Path) -> Result<String, CollectionError> {
    let relative = file.strip_prefix("/").map_err(|_| {
        CollectionError::new(format!(
            "{} is a relative path, which the server resolved against a working directory \
             nothing records",
            file.display()
        ))
    })?;

    read_inside(root, relative).map_err(|error| {
        CollectionError::new(format!("{} could not be read: {error}", file.display()))
    })
}

/// A line's arguments, nothing for a blank line or a comment, or a failure where it will not
/// split the way the server splits it.
fn words_of(file: &Path, line: &str) -> Result<Option<Vec<String>>, CollectionError> {
    let line = line.trim_matches([' ', '\t', '\r', '\n']);
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }

    split_arguments(line).map(Some).ok_or_else(|| {
        CollectionError::new(format!(
            "{} has a line rastro cannot split the way the server does: unbalanced quotes, or a \
             byte escape outside ASCII",
            file.display()
        ))
    })
}

/// The rules of a `user default …` line; the directive is matched in any case, the account name
/// exactly, as the server matches them.
fn default_user_rules(words: &[String]) -> Option<Vec<String>> {
    match words {
        [directive, name, rules @ ..]
            if directive.eq_ignore_ascii_case(USER) && name == DEFAULT_USER =>
        {
            Some(rules.to_vec())
        }
        _ => None,
    }
}

fn declared_twice(file: &Path) -> CollectionError {
    CollectionError::new(format!(
        "{} declares the default account twice, which the server refuses to start with",
        file.display()
    ))
}

/// The files one `include` names, in the order the server reads them.
///
/// A relative path is refused rather than resolved: the server resolves it against its working
/// directory at start, which nothing on the box still records.
fn included_files(
    root: &Path,
    from: &Path,
    argument: &str,
) -> Result<Vec<PathBuf>, CollectionError> {
    let path = Path::new(argument);
    if path.is_relative() {
        return Err(CollectionError::new(format!(
            "{} includes {argument:?} by a relative path, which the server resolved against a \
             working directory nothing records",
            from.display()
        )));
    }
    if !file_glob::is_pattern(path) {
        return Ok(vec![path.to_path_buf()]);
    }

    // A wildcard in the last component alone, the drop-in directory every packaged layout uses.
    let (Some(directory), Some(pattern)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(unresolved_pattern(from, argument));
    };
    if file_glob::is_pattern(directory) || pattern.contains('[') {
        return Err(unresolved_pattern(from, argument));
    }

    let relative = directory.strip_prefix("/").unwrap_or(directory);
    let mut names = names_inside(root, relative).map_err(|error| {
        CollectionError::new(format!(
            "{} includes {argument:?}, and {} could not be listed: {error}",
            from.display(),
            directory.display()
        ))
    })?;
    names.retain(|name| file_glob::name_matches(name, pattern));
    // Byte order, as `file_glob` sorts, so the include order is the same under every locale.
    names.sort();

    Ok(names.into_iter().map(|name| directory.join(name)).collect())
}

fn unresolved_pattern(from: &Path, argument: &str) -> CollectionError {
    CollectionError::new(format!(
        "{} includes {argument:?}, a pattern rastro does not resolve: a wildcard outside the last \
         component, or a bracket expression",
        from.display()
    ))
}

/// Splits a line into its arguments as redis's `sdssplitargs` does, or nothing where the quotes
/// do not balance.
///
/// A double-quoted argument takes `\xHH`, `\n`, `\r`, `\t`, `\b`, `\a` and a backslash before any
/// other character as that character; a single-quoted one takes `\'` and nothing else. A quote
/// inside an unquoted word opens a quoted part of the same word, and a closing quote must be
/// followed by a space or the end of the line.
fn split_arguments(line: &str) -> Option<Vec<String>> {
    let mut arguments = Vec::new();
    let mut characters = line.chars().peekable();

    loop {
        while characters
            .next_if(|character| is_c_space(*character))
            .is_some()
        {}
        if characters.peek().is_none() {
            return (!arguments.is_empty()).then_some(arguments);
        }

        let mut argument = String::new();
        loop {
            match characters.next() {
                None => break,
                Some(character) if is_c_space(character) => break,
                Some('"') => {
                    double_quoted(&mut characters, &mut argument)?;
                    if characters.peek().is_some_and(|next| !is_c_space(*next)) {
                        return None;
                    }
                }
                Some('\'') => {
                    single_quoted(&mut characters, &mut argument)?;
                    if characters.peek().is_some_and(|next| !is_c_space(*next)) {
                        return None;
                    }
                }
                Some(character) => argument.push(character),
            }
        }

        arguments.push(argument);
    }
}

/// Whether a character separates words, as C's `isspace` decides it in redis: ASCII only, so a
/// non-breaking space inside a password stays inside it.
fn is_c_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}

fn double_quoted(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    argument: &mut String,
) -> Option<()> {
    loop {
        match characters.next()? {
            '"' => return Some(()),
            '\\' => match characters.next()? {
                // A byte above 0x7F is a raw byte to redis and would go out as two in UTF-8,
                // which is a wrong password, so it is refused rather than approximated.
                // `\x` takes two hex digits, and without them is the letter `x`, as in redis.
                'x' => {
                    let mut ahead = characters.clone();
                    match (
                        ahead.next().and_then(|digit| digit.to_digit(16)),
                        ahead.next().and_then(|digit| digit.to_digit(16)),
                    ) {
                        (Some(high), Some(low)) => {
                            characters.next();
                            characters.next();
                            let byte = u8::try_from(high * 16 + low).ok().filter(u8::is_ascii)?;
                            argument.push(char::from(byte));
                        }
                        _ => argument.push('x'),
                    }
                }
                'n' => argument.push('\n'),
                'r' => argument.push('\r'),
                't' => argument.push('\t'),
                'b' => argument.push('\u{8}'),
                'a' => argument.push('\u{7}'),
                other => argument.push(other),
            },
            other => argument.push(other),
        }
    }
}

fn single_quoted(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    argument: &mut String,
) -> Option<()> {
    loop {
        match characters.next()? {
            '\'' => return Some(()),
            '\\' if characters.peek() == Some(&'\'') => {
                characters.next();
                argument.push('\'');
            }
            other => argument.push(other),
        }
    }
}
