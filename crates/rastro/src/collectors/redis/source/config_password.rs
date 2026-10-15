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
use crate::collectors::inside_root::ProcessRoot;

const REQUIREPASS: &str = "requirepass";
const INCLUDE: &str = "include";
const USER: &str = "user";
const ACLFILE: &str = "aclfile";

/// The account `requirepass` sets, and the only one whose password rastro ever needs.
const DEFAULT_USER: &str = "default";

/// The most files and `include` lines one configuration is read to, includes and their includes
/// together.
///
/// A budget of work rather than a depth: depth alone bounds nesting, and fifteen files each
/// including the next three times took 35 s, measured. It also stops a file that includes itself,
/// directly or round a loop, without a set of open files. A real configuration names a handful.
/// **Include lines count too**, found by review: a wildcard matching nothing reads no file, and a
/// mebibyte of them over a wide directory took 172 s.
const MOST_FILES: usize = 64;

/// The most names one configuration's wildcard includes are matched against, together: each
/// listing is bounded on its own, and a real drop-in directory holds a handful.
const MOST_NAMES: usize = 10_000;

/// The longest pattern matched, `NAME_MAX`: no name is longer, so no longer pattern matches one,
/// and a pattern of a megabyte took 92 s, found by review.
const LONGEST_PATTERN: usize = 255;

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
    root: &ProcessRoot,
    file: &Path,
) -> Result<PasswordDirectives, CollectionError> {
    let mut directives = PasswordDirectives::default();
    read_into(root, file, &mut Budget::default(), &mut directives)?;
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
    root: &ProcessRoot,
    file: &Path,
) -> Result<Option<Vec<String>>, CollectionError> {
    let bytes = read_bounded(root, file)?;

    let mut default_user = None;
    for line in bytes.split(|byte| *byte == b'\n') {
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

/// What one file, its includes with it, last set the password and the ACL file to.
#[derive(Default)]
struct LastSet {
    requirepass: Option<String>,
    acl_file: Option<PathBuf>,
}

impl LastSet {
    fn then(&mut self, later: LastSet) {
        self.requirepass = later.requirepass.or(self.requirepass.take());
        self.acl_file = later.acl_file.or(self.acl_file.take());
    }
}

/// What reading one configuration has spent so far.
#[derive(Default)]
struct Budget {
    /// Files read and `include` lines met.
    steps: usize,

    /// Names wildcard includes were matched against.
    names: usize,
}

impl Budget {
    fn step(&mut self, file: &Path) -> Result<(), CollectionError> {
        self.steps += 1;
        match self.steps > MOST_FILES {
            true => Err(CollectionError::new(format!(
                "{} is past the {MOST_FILES} files and includes a configuration is read to, which \
                 is a file including itself or an include tree wider than any real one",
                file.display()
            ))),
            false => Ok(()),
        }
    }

    fn match_names(&mut self, count: usize, from: &Path) -> Result<(), CollectionError> {
        self.names += count;
        match self.names > MOST_NAMES {
            true => Err(CollectionError::new(format!(
                "{} includes patterns matched against more than {MOST_NAMES} names together, \
                 more than any real drop-in directory holds",
                from.display()
            ))),
            false => Ok(()),
        }
    }
}

/// Reads `file` into `directives`, saying what it set.
fn read_into(
    root: &ProcessRoot,
    file: &Path,
    budget: &mut Budget,
    directives: &mut PasswordDirectives,
) -> Result<LastSet, CollectionError> {
    budget.step(file)?;

    let bytes = read_bounded(root, file)?;
    let mut set = LastSet::default();

    for line in bytes.split(|byte| *byte == b'\n') {
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
            (REQUIREPASS, 2) => {
                directives.requirepass = Some(words[1].clone());
                set.requirepass = Some(words[1].clone());
            }
            (ACLFILE, 2) => {
                directives.acl_file = Some(PathBuf::from(&words[1]));
                set.acl_file = Some(PathBuf::from(&words[1]));
            }
            (INCLUDE, 2) => {
                budget.step(file)?;
                let mut matches = Vec::new();
                for included in included_files(root, file, &words[1], budget)? {
                    matches.push(read_into(root, &included, budget, directives)?);
                }
                if disagree(&matches) {
                    return Err(CollectionError::new(format!(
                        "{} includes {:?}, whose files set different passwords, and which the \
                         server read last depends on the locale it collated their names in",
                        file.display(),
                        words[1]
                    )));
                }
                for included in matches {
                    set.then(included);
                }
            }
            _ => {}
        }
    }

    Ok(set)
}

/// Whether the files one `include` matched set the password or the ACL file to more than one
/// value, so that the one that wins depends on the order they were read in.
///
/// Found by review: the server reads a pattern's matches in `glob`'s order, which its locale
/// collates, and `redis.conf` against `redis-extra.conf` sorts one way in C and the other in
/// en_US.UTF-8. rastro reads them in byte order, which only agreement makes safe.
fn disagree(matches: &[LastSet]) -> bool {
    let distinct = |values: Vec<String>| {
        values
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1
    };

    distinct(
        matches
            .iter()
            .filter_map(|set| set.requirepass.clone())
            .collect(),
    ) || distinct(
        matches
            .iter()
            .filter_map(|set| set.acl_file.as_ref().map(|path| path.display().to_string()))
            .collect(),
    )
}

/// A file's bytes, read bounded and only where it is a regular file.
///
/// The file is the redis account's, Debian's `redis.conf` among them, so what is at the path is
/// that account's choice: a FIFO there blocked the run and `/dev/zero` grew without end, the
/// finding the elasticsearch collector met first.
///
/// **Bytes, not text**, found by review: redis reads the file as bytes, and one Latin-1 comment
/// would otherwise refuse every password in it.
fn read_bounded(root: &ProcessRoot, file: &Path) -> Result<Vec<u8>, CollectionError> {
    let relative = file.strip_prefix("/").map_err(|_| {
        CollectionError::new(format!(
            "{} is a relative path, which the server resolved against a working directory \
             nothing records",
            file.display()
        ))
    })?;

    root.read_bytes(relative).map_err(|error| {
        CollectionError::new(format!("{} could not be read: {error}", file.display()))
    })
}

/// A line's arguments, nothing for a blank line, a comment or a line that is not UTF-8 and is none
/// of the directives read, or a failure where it will not split the way the server splits it.
///
/// **Only a line the password may depend on has to be UTF-8**: its arguments are sent as text, and
/// a raw byte redis would send as itself would go out as two. A quoted first word may be any
/// directive, so it is not skipped.
fn words_of(file: &Path, line: &[u8]) -> Result<Option<Vec<String>>, CollectionError> {
    // redis trims these four alone, `sdstrim(line, " \t\r\n")`.
    let trimmed = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
    let start = line
        .iter()
        .position(|byte| !trimmed(byte))
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|byte| !trimmed(byte))
        .map_or(start, |last| last + 1);
    let line = &line[start..end];
    if line.is_empty() || line.starts_with(b"#") {
        return Ok(None);
    }
    // redis's reader stops a line at a NUL, where splitting reads on past it, found by review.
    if line.contains(&0) && may_be_read(line) {
        return Err(CollectionError::new(format!(
            "{} has a line the password may depend on that holds a NUL, which the server reads \
             differently from any splitting of the line",
            file.display()
        )));
    }
    let line = match std::str::from_utf8(line) {
        Ok(line) => line,
        Err(_) if !may_be_read(line) => return Ok(None),
        Err(_) => {
            return Err(CollectionError::new(format!(
                "{} has a line the password may depend on that is not UTF-8, so it cannot be \
                 sent as the server would send it",
                file.display()
            )));
        }
    };

    // No word at all, a line of form feeds, is one the server passes over.
    split_arguments(line)
        .map(|words| (!words.is_empty()).then_some(words))
        .ok_or_else(|| {
        CollectionError::new(format!(
            "{} has a line rastro cannot split the way the server does: unbalanced quotes, or a \
             byte escape outside ASCII",
            file.display()
        ))
    })
}

/// Whether a line's first word may be one of the directives read, compared as bytes.
///
/// The word is found as `sdssplitargs` finds it, found by review: past any C `isspace`, a vertical
/// tab or form feed among them, which Rust's ASCII whitespace leaves out or keeps.
fn may_be_read(line: &[u8]) -> bool {
    let directive = line
        .split(|byte| *byte == 0 || is_c_space(char::from(*byte)))
        .find(|word| !word.is_empty())
        .unwrap_or_default()
        .to_ascii_lowercase();

    directive.contains(&b'"')
        || directive.contains(&b'\'')
        || [REQUIREPASS, INCLUDE, USER, ACLFILE]
            .iter()
            .any(|read| read.as_bytes() == directive.as_slice())
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
    root: &ProcessRoot,
    from: &Path,
    argument: &str,
    budget: &mut Budget,
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
        path.file_name().and_then(std::ffi::OsStr::to_str),
    ) else {
        return Err(unresolved_pattern(from, argument));
    };
    if file_glob::is_pattern(directory) || pattern.contains(['[', '\\']) {
        return Err(unresolved_pattern(from, argument));
    }
    if pattern.len() > LONGEST_PATTERN {
        return Err(CollectionError::new(format!(
            "{} includes a pattern longer than any file name can be, which matches nothing the \
             server could have read",
            from.display()
        )));
    }

    let relative = directory.strip_prefix("/").unwrap_or(directory);
    let mut names = root.names(relative).map_err(|error| {
        CollectionError::new(format!(
            "{} includes {argument:?}, and {} could not be listed: {error}",
            from.display(),
            directory.display()
        ))
    })?;
    budget.match_names(names.len(), from)?;
    names.retain(|name| file_glob::name_matches(name, pattern));
    // Found by review: the listing spells a name that is not UTF-8 lossily, which can collide with
    // a real file's, so the file the server read would be read as another.
    if names
        .iter()
        .any(|name| name.contains(char::REPLACEMENT_CHARACTER))
    {
        return Err(CollectionError::new(format!(
            "{} includes {argument:?}, and {} holds a matching name that is not UTF-8, which \
             rastro cannot tell from another",
            from.display(),
            directory.display()
        )));
    }
    // Byte order, as `file_glob` sorts, so the include order is the same under every locale.
    names.sort();

    Ok(names.into_iter().map(|name| directory.join(name)).collect())
}

fn unresolved_pattern(from: &Path, argument: &str) -> CollectionError {
    CollectionError::new(format!(
        "{} includes {argument:?}, a pattern rastro does not resolve: a wildcard outside the last \
         component, a bracket expression or an escape",
        from.display()
    ))
}

/// Splits a line into its arguments as redis's `sdssplitargs` does, none for a line holding no
/// word, or nothing where the quotes do not balance.
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
            return Some(arguments);
        }

        let mut argument = String::new();
        loop {
            match characters.next() {
                None => break,
                Some(character) if ends_a_word(character) => break,
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

/// Whether a character is skipped between words, as C's `isspace` decides it in redis: ASCII only,
/// so a non-breaking space inside a password stays inside it.
fn is_c_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}

/// Whether a character ends an unquoted word, which `sdssplitargs` lists apart from `isspace`: a
/// vertical tab or a form feed stays in the word.
fn ends_a_word(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\0')
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
