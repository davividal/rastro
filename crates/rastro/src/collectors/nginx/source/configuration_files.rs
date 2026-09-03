//! The configuration as it lies on disk, assembled the way nginx assembles it.
//!
//! One `include` at a time, depth first, exactly where the directive stood. What this does
//! not do is run nginx: see the module documentation for why testing a configuration is not
//! a read.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::nginx::value_objects::{ConfigurationSource, SecondsSinceEpoch};

use super::conf_syntax;
use super::file_glob;
use crate::collectors::nginx::model::{Configuration, ConfigurationFile, Directive};

/// The directive that pulls another file in.
const INCLUDE: &str = "include";

/// Where a configuration starts, and what its relative paths are relative to.
///
/// Both are absolute by construction, because everything below is a join onto them: a
/// relative prefix would make every recorded path relative to whatever directory the run
/// happened to start in, which is not a fact about the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationFiles {
    root: PathBuf,
    prefix: PathBuf,
    chosen_by: ConfigurationSource,
}

impl ConfigurationFiles {
    pub fn at(
        root: impl Into<PathBuf>,
        prefix: impl Into<PathBuf>,
        chosen_by: ConfigurationSource,
    ) -> Result<Self, CollectionError> {
        let prefix = absolute(prefix.into(), "nginx prefix")?;
        // nginx resolves a relative `-c` against the prefix, so this does too.
        let root = absolute(prefix.join(root.into()), "nginx configuration path")?;

        Ok(Self {
            root,
            prefix,
            chosen_by,
        })
    }

    /// Reads every file nginx would read, recording each one's fate.
    ///
    /// No failure reaches the caller, because none of them is a failure of the run: a file
    /// that cannot be read is recorded as refused and the rest of the configuration is still
    /// worth having. The one thing this cannot report is a configuration whose root is
    /// missing, and it does not have to — that file is in the record like any other.
    pub fn read(&self) -> Configuration {
        let mut reading = Reading {
            prefix: &self.prefix,
            files: Vec::new(),
            open: Vec::new(),
            newest: None,
        };
        let directives = reading.visit(&self.root);

        Configuration {
            prefix: recorded(&self.prefix),
            root: recorded(&self.root),
            files: reading.files,
            directives,
            chosen_by: self.chosen_by,
            newest_modified: reading.newest.map(SecondsSinceEpoch::new),
        }
    }
}

fn absolute(path: PathBuf, kind: &str) -> Result<PathBuf, CollectionError> {
    match path.is_absolute() {
        true => Ok(path),
        false => Err(CollectionError::new(format!(
            "the {kind} {} is relative, and a fingerprint records where a file is rather \
             than where the run started",
            path.display()
        ))),
    }
}

/// One pass over the configuration, carrying what has been read and what is still open.
struct Reading<'a> {
    prefix: &'a Path,
    files: Vec<ConfigurationFile>,
    /// The files on the current include stack, resolved through symlinks.
    ///
    /// Resolved, because `sites-enabled/site` and `sites-available/site` are the same file
    /// under two names and a cycle through the pair would otherwise never be seen. A file
    /// included twice from *different* branches is not a cycle and is read twice, which is
    /// what nginx does.
    open: Vec<PathBuf>,
    /// The newest mtime seen among the files that were read.
    newest: Option<i64>,
}

impl Reading<'_> {
    fn visit(&mut self, path: &Path) -> Vec<Directive> {
        let identity = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if self.open.contains(&identity) {
            self.refuse(
                path,
                format!(
                    "{} includes itself, and rastro stops where nginx would recurse",
                    path.display()
                ),
            );
            return Vec::new();
        }

        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                self.refuse(
                    path,
                    format!("{} could not be read: {error}", path.display()),
                );
                return Vec::new();
            }
        };

        let directives = match conf_syntax::parse(&text) {
            Ok(directives) => directives,
            Err(error) => {
                self.refuse(
                    path,
                    format!("{} is not a configuration: {error}", path.display()),
                );
                return Vec::new();
            }
        };

        self.files
            .push(ConfigurationFile::parsed(recorded(path), &directives));
        self.note_change(path);

        self.open.push(identity);
        let expanded = self.expanded(directives);
        self.open.pop();

        expanded
    }

    /// The same directives with every `include` replaced by what it names.
    fn expanded(&mut self, directives: Vec<Directive>) -> Vec<Directive> {
        let mut expanded = Vec::new();

        for directive in directives {
            if directive.name.as_str() == INCLUDE && directive.block.is_none() {
                expanded.extend(self.included(&directive));
                continue;
            }

            expanded.push(Directive {
                block: directive.block.map(|block| self.expanded(block)),
                ..directive
            });
        }

        expanded
    }

    fn included(&mut self, include: &Directive) -> Vec<Directive> {
        let mut found = Vec::new();

        for argument in &include.arguments {
            let named = self.prefix.join(argument.as_str());

            if !file_glob::is_pattern(&named) {
                found.extend(self.visit(&named));
                continue;
            }

            match file_glob::matching(&named) {
                Ok(paths) => {
                    for path in paths {
                        found.extend(self.visit(&path));
                    }
                }
                Err(error) => self.refuse(&named, error.to_string()),
            }
        }

        found
    }

    /// Remembers the file's mtime if it is the newest one read so far.
    ///
    /// A file that cannot be stat'ed contributes nothing rather than failing the read: it was
    /// just parsed, so it exists, and a race that removes it between the two is not worth a
    /// facet.
    fn note_change(&mut self, path: &Path) {
        if let Ok(metadata) = fs::metadata(path) {
            let modified = metadata.mtime();
            self.newest = Some(self.newest.map_or(modified, |newest| newest.max(modified)));
        }
    }

    fn refuse(&mut self, path: &Path, reason: String) {
        let reason = NonEmptyText::new(reason, "configuration file refusal")
            .expect("every refusal above says something");
        self.files
            .push(ConfigurationFile::refused(recorded(path), reason));
    }
}

/// The path as the document carries it.
fn recorded(path: &Path) -> AbsolutePath {
    AbsolutePath::new(path.to_string_lossy(), "nginx configuration file")
        .expect("every path here is a join onto an absolute prefix")
}
