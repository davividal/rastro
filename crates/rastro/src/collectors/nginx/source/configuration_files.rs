//! The configuration as it lies on disk, assembled the way nginx assembles it.
//!
//! One `include` at a time, depth first, exactly where the directive stood. What this does
//! not do is run nginx: see the module documentation for why testing a configuration is not
//! a read.
//!
//! **nginx has two bases for a relative path, and using one for both is wrong on Debian.**
//! `prefix` is `-p`, or `--prefix` at build time, and it is what a cache or a temp path
//! resolves against. `conf_prefix` is the *directory of the configuration file* — `-c`'s, or
//! `--conf-path`'s — and it is what an `include`, a certificate and a user file resolve
//! against. Debian builds nginx with `--prefix=/usr/share/nginx` and
//! `--conf-path=/etc/nginx/nginx.conf`, so the two are different directories and a collector
//! that knew only the first would look for every included file in a directory that holds
//! none.
//!
//! Measured, on nginx 1.30 started as `-p /tmp/altprefix -c /etc/nginx/nginx.conf`: a request
//! against a location with `auth_basic_user_file relative.htpasswd` logged
//! `open() "/etc/nginx/relative.htpasswd" failed`. nginx derives `conf_prefix` by taking the
//! directory of the configuration file it ended up with, which is what this does.

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
    /// The directory of the root configuration file, which is what an `include` resolves
    /// against.
    configuration_prefix: PathBuf,
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

        let configuration_prefix = root
            .parent()
            .map_or_else(|| prefix.clone(), Path::to_path_buf);

        Ok(Self {
            root,
            prefix,
            configuration_prefix,
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
            configuration_prefix: &self.configuration_prefix,
            files: Vec::new(),
            open: Vec::new(),
            newest: None,
        };
        let directives = reading.visit(&self.root);

        Configuration {
            prefix: recorded(&self.prefix),
            configuration_prefix: recorded(&self.configuration_prefix),
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
    configuration_prefix: &'a Path,
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

        self.record(ConfigurationFile::parsed(recorded(path), &directives));
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
            let named = self.configuration_prefix.join(argument.as_str());

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

    /// Records a file the first time it is read, and not again.
    ///
    /// **A file included from two places is read twice and recorded once**, which is what
    /// `nginx -T` does with its own dump and is the right answer for the same reason: this
    /// list says which files make up the configuration, and a second identical entry with an
    /// identical digest answers nothing a reader asked. The directives are still expanded
    /// both times, because nginx applies them both times — measured, on nginx 1.26: a
    /// `server` block in a file included twice produces the `conflicting server name`
    /// warning, which only two server blocks can produce.
    fn record(&mut self, file: ConfigurationFile) {
        if self.files.iter().any(|recorded| recorded.path == file.path) {
            return;
        }

        self.files.push(file);
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
        self.record(ConfigurationFile::refused(recorded(path), reason));
    }
}

/// The path as the document carries it.
fn recorded(path: &Path) -> AbsolutePath {
    AbsolutePath::new(path.to_string_lossy(), "nginx configuration file")
        .expect("every path here is a join onto an absolute prefix")
}
