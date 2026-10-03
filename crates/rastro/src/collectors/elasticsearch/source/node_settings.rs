//! The settings a node was given at start: its argv, its environment and its own file.
//!
//! **Read only to decide whether and where the node may be asked**, which is the one reason
//! this collector parses a configuration file. The node's answer over its API is the
//! authoritative one, and the dispatch cannot have it yet: which port serves HTTP, and whether
//! that port wants TLS, have to be known before the first request, or the first request is a
//! guess. See `docs/decisions.md`.
//!
//! Three sources, in the precedence the node applies, **which depends on how it was installed**,
//! measured rather than read from the documentation:
//!
//! - **the docker distribution**: an environment variable named after the setting, dots and all,
//!   over a `-E` flag, over `elasticsearch.yml`. The domain review measured the variable winning
//!   on the 7.17.24, 8.15.3 and 9.2.0 images with all three set, and from 8.x the image's
//!   settings appear nowhere in the argv;
//! - **every other distribution**: a `-E` flag over the file, and **the environment holds no
//!   settings at all**. Measured on the 7.17.24 and 8.15.3 tarballs: with a dotted variable set,
//!   the node ran on its file's value.
//!
//! Which one a node is comes from `es.distribution.type` on its launch argv, and a node that names
//! none is refused, since the two readings can disagree on the very port or protocol it is asked
//! on. The flags are on the argv the node was launched with, which on 8.x is its launcher's.
//!
//! Everything is read through `/proc/<pid>`, the file included, as `root/<es.path.conf>`: a
//! node in a container reads the file in its own image, and the host's `/etc/elasticsearch`,
//! if there is one, may belong to a different node.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use yaml_rust2::{Yaml, YamlLoader};

use crate::collectors::elasticsearch::source::ResidentNode;
use crate::collectors::elasticsearch::source::in_root::{changed_at_inside, read_inside};
use crate::collectors::elasticsearch::value_objects::{Transport, Unread};

/// The argument vector's separator, and the environment's, which is how the kernel writes both.
const SEPARATOR: u8 = b'\0';

/// The prefix a setting on the command line carries.
const COMMAND_LINE_SETTING: &str = "-E";

const CONFIG_FILE: &str = "elasticsearch.yml";

/// The one distribution whose environment holds settings.
const DOCKER_DISTRIBUTION: &str = "docker";

/// The server's options that take no value, from its own `--help`.
const FLAG_OPTIONS: [&str; 12] = [
    "-d",
    "--daemonize",
    "-q",
    "--quiet",
    "-s",
    "--silent",
    "-v",
    "--verbose",
    "-V",
    "--version",
    "-h",
    "--help",
];

/// The server's options whose value is the next argument.
const VALUE_OPTIONS: [&str; 3] = ["-p", "--pidfile", "--enrollment-token"];

/// The same options with their value joined: `-p/run/es.pid`, `-p=…`, `--pidfile=…`.
const JOINED_VALUE_OPTIONS: [&str; 3] = ["-p", "--pidfile=", "--enrollment-token="];

/// How long after the node's start a change to its file is still the node's own start-up write,
/// in seconds. Auto-configuration measured at 0.67 s; the rest is room for a slow box.
const START_UP_WRITE_WINDOW: i64 = 60;

/// The prefix of a setting's encoded name, for environments that cannot put dots in a name.
const ENCODED_SETTING_PREFIX: &str = "ES_SETTING_";

/// Where a node keeps its data, one directory or a comma-joined list of them.
const DATA_PATH: &str = "path.data";

/// Where a node keeps its data when nothing says, relative to its home.
const DEFAULT_DATA_DIRECTORY: &str = "data";

/// The setting that puts the HTTP listener behind TLS.
const TLS_SETTING: &str = "xpack.security.http.ssl.enabled";

/// The setting that makes the node ask every request for credentials.
const SECURITY_SETTING: &str = "xpack.security.enabled";

/// The setting that makes the node record every request it receives.
const AUDIT_SETTING: &str = "xpack.security.audit.enabled";

/// A node's start-up settings, flattened to dotted keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeSettings {
    /// Each value as the node would read it, a list joined with commas the way Elasticsearch
    /// accepts one for a list setting.
    values: BTreeMap<String, String>,
}

impl NodeSettings {
    /// Reads a node's settings through the box's `/proc`.
    pub fn read(node: &ResidentNode) -> Result<Self, Unread> {
        Self::read_in(Path::new("/proc"), node)
    }

    /// The same through a process table the caller names.
    pub fn read_in(proc: &Path, node: &ResidentNode) -> Result<Self, Unread> {
        let process = proc.join(node.process_id().to_string());
        if !node.launch_arguments_are_exact() {
            return Err(Unread::new(
                "the node's argv holds an argument that is not UTF-8, so its paths and \
                 command-line settings cannot be read exactly as the node reads them",
            ));
        }
        if node.launched_with_an_argument_file() {
            return Err(Unread::new(
                "the node was launched with a java argument file that could not be read, which \
                 can set its paths and settings where /proc does not show them",
            ));
        }
        let config = node.config().ok_or_else(|| {
            Unread::new(
                "the node's argv names no es.path.conf, so its elasticsearch.yml cannot be \
                 located without guessing",
            )
        })?;

        let distribution = node.distribution().ok_or_else(|| {
            Unread::new(
                "the node's argv names no es.distribution.type, so whether its environment \
                 holds settings cannot be told",
            )
        })?;

        // Read on every distribution: `${NAME}` in the file is resolved from it wherever the
        // node was installed, even where the variables are not settings themselves.
        let environment = read_pairs(&process.join("environ"), "environ")?;
        let started_at = node.started_at().ok_or_else(|| {
            Unread::new(
                "when the node started cannot be read, so whether its elasticsearch.yml changed \
                 since cannot be told",
            )
        })?;
        let file = read_config_file(&process.join("root"), config, &environment, started_at)?;
        let command_line = command_line_settings(node.application_arguments())?;

        let mut values = file;
        values.extend(command_line);
        if distribution == DOCKER_DISTRIBUTION {
            values.extend(environment_settings(&environment)?);
        }

        Ok(Self { values })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The directories the node keeps its data in, as paths in its own mount namespace.
    ///
    /// `path.data` where it is set, each entry of a list separately and a relative one against
    /// the home, which is how the node resolves it; `data` under the home otherwise. Nothing
    /// where neither the setting nor the home is known, since a guessed path would seal a tree
    /// that is not the node's.
    pub fn data_directories(&self, home: Option<&Path>) -> Vec<PathBuf> {
        let resolved = |directory: &str| {
            let directory = Path::new(directory);
            match (directory.is_absolute(), home) {
                (true, _) => Some(directory.to_path_buf()),
                (false, Some(home)) => Some(home.join(directory)),
                (false, None) => None,
            }
        };

        match self.get(DATA_PATH) {
            Some(listed) => listed
                .split(',')
                .map(str::trim)
                .filter(|directory| !directory.is_empty())
                .filter_map(resolved)
                .collect(),
            None => resolved(DEFAULT_DATA_DIRECTORY).into_iter().collect(),
        }
    }

    /// Plain only where the TLS setting is absent or exactly `false`.
    ///
    /// A value the node would reject as a boolean is read as TLS, because being wrong that way
    /// costs an unread facet and being wrong the other way costs a request the node refused.
    pub fn transport(&self) -> Transport {
        match self.get(TLS_SETTING) {
            None | Some("false") => Transport::Plain,
            Some(_) => Transport::TlsRequired,
        }
    }

    /// Whether the settings switch security on, so a request without credentials is refused.
    ///
    /// Read the way the TLS setting is, anything but absent or exactly `false` counting as on.
    /// Absent is dialled: it is off on 7.x, and where an 8.x default leaves it on the node answers
    /// 401 and records nothing unless audit logging, which is never on by default, says so.
    pub fn asks_for_credentials(&self) -> bool {
        switched_on(self.get(SECURITY_SETTING))
    }

    /// Whether the settings switch audit logging on, so any request received is recorded.
    pub fn audits_requests(&self) -> bool {
        switched_on(self.get(AUDIT_SETTING))
    }
}

fn switched_on(value: Option<&str>) -> bool {
    !matches!(value, None | Some("false"))
}

/// The `-E` settings among the server's arguments, every argument placed or the node refused.
///
/// **A closed set, found by review the third time a spelling was missed.** `-Ename=value`, then
/// `-E name=value`, then `-E=name=value` each read as no setting at all, and each let a node whose
/// TLS was switched on that way be sent plaintext. The server's options, from its own `--help` on
/// 7.17.24, 8.15.3 and 9.2.0, are few, so each is placed in every spelling jopt-simple takes, a
/// value joined, after `=` or as the next argument, and anything else refuses the node: a
/// spelling not yet known is a refusal rather than a misreading.
fn command_line_settings(arguments: &[String]) -> Result<Vec<(String, String)>, Unread> {
    let mut settings = Vec::new();
    let mut rest = arguments.iter();
    let unplaced = |argument: &str| {
        Unread::new(format!(
            "the node's command line holds `{argument}`, which rastro cannot place, so its \
             settings cannot be read exactly"
        ))
    };

    while let Some(argument) = rest.next() {
        let argument = argument.as_str();
        if FLAG_OPTIONS.contains(&argument) {
            continue;
        }
        if VALUE_OPTIONS.contains(&argument) {
            rest.next().ok_or_else(|| unplaced(argument))?;
            continue;
        }
        if JOINED_VALUE_OPTIONS
            .iter()
            .any(|option| argument.starts_with(option) && argument.len() > option.len())
        {
            continue;
        }

        let setting = match argument.strip_prefix(COMMAND_LINE_SETTING) {
            Some("") => rest
                .next()
                .map(String::as_str)
                .ok_or_else(|| unplaced(argument))?,
            Some(joined) => joined.strip_prefix('=').unwrap_or(joined),
            None => return Err(unplaced(argument)),
        };
        let (name, value) = setting
            .split_once('=')
            .filter(|(name, _)| !name.is_empty())
            .ok_or_else(|| unplaced(argument))?;
        settings.push((name.to_owned(), value.to_owned()));
    }

    Ok(settings)
}

/// The settings the docker image takes from its environment, in both spellings it accepts.
///
/// A variable named after the setting, dots and all, and `ES_SETTING_` followed by the name in
/// capitals with each dot an underscore and each underscore doubled: measured on the 7.17.24 and
/// 8.15.3 images, `ES_SETTING_NODE_ATTR_RACK__ID=r1` became `node.attr.rack_id`. Found by review,
/// after the encoded form had been ignored and audit logging switched on through it read as off.
/// Nothing measured says which spelling wins where both name one setting, so two that disagree
/// are a refusal.
fn environment_settings(
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Unread> {
    let mut settings: BTreeMap<String, String> = BTreeMap::new();

    for (variable, value) in environment {
        let name = match variable.strip_prefix(ENCODED_SETTING_PREFIX) {
            Some(encoded) => decoded(encoded),
            None if variable.contains('.') => variable.clone(),
            None => continue,
        };

        if let Some(earlier) = settings.get(&name)
            && earlier != value
        {
            return Err(Unread::new(format!(
                "the node's environment sets {name} twice, to `{earlier}` and `{value}`, and \
                 which one the node took cannot be told"
            )));
        }
        settings.insert(name, value.clone());
    }

    Ok(settings)
}

/// A setting's name from its `ES_SETTING_` spelling: `__` is an underscore, `_` a dot.
fn decoded(encoded: &str) -> String {
    encoded
        .split("__")
        .map(|part| part.replace('_', "."))
        .collect::<Vec<_>>()
        .join("_")
        .to_lowercase()
}

/// Settings the caller already has, so what depends on them can be exercised without a node.
impl FromIterator<(String, String)> for NodeSettings {
    fn from_iter<Pairs: IntoIterator<Item = (String, String)>>(pairs: Pairs) -> Self {
        Self {
            values: pairs.into_iter().collect(),
        }
    }
}

/// The environment's variables, read entry by entry as bytes.
///
/// One variable that is not UTF-8 used to fail the whole read, over a value the collector never
/// looks at. Now such a variable is skipped, unless its name is a setting's: a setting rastro
/// cannot read exactly is one it may not act on, so that is still a refusal.
fn read_pairs(path: &Path, what: &str) -> Result<BTreeMap<String, String>, Unread> {
    let raw = fs::read(path)
        .map_err(|error| Unread::new(format!("the node's {what} could not be read: {error}")))?;

    let mut pairs = BTreeMap::new();
    for entry in raw
        .split(|byte| *byte == SEPARATOR)
        .filter(|entry| !entry.is_empty())
    {
        match std::str::from_utf8(entry) {
            Ok(text) => {
                if let Some((name, value)) = text.split_once('=') {
                    pairs.insert(name.to_owned(), value.to_owned());
                }
            }
            Err(_) => {
                let name = entry.split(|byte| *byte == b'=').next().unwrap_or_default();
                if let Ok(name) = std::str::from_utf8(name)
                    && name.contains('.')
                {
                    return Err(Unread::new(format!(
                        "the setting {name} in the node's {what} is not UTF-8, so it cannot be \
                         read exactly as the node reads it"
                    )));
                }
            }
        }
    }

    Ok(pairs)
}

/// The file's settings, or none where the directory holds no file.
///
/// A missing file is a node on its defaults and the other two sources, which is how it
/// started; a file that is there and cannot be read is a refusal.
fn read_config_file(
    root: &Path,
    config: &Path,
    environment: &BTreeMap<String, String>,
    started_at: u64,
) -> Result<BTreeMap<String, String>, Unread> {
    let named = config.join(CONFIG_FILE);
    let relative = named.strip_prefix("/").unwrap_or(&named);

    // Measured on 8.15.3: security auto-configuration writes the file 0.67 s after the server
    // starts, and the node runs with that write, so a change soon after start is the node's own.
    let started = i64::try_from(started_at).unwrap_or(i64::MAX);
    let changed_since_start =
        |changed_at: i64| changed_at > started.saturating_add(START_UP_WRITE_WINDOW);

    let file = match read_inside(root, relative) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return no_file_since_start(root, relative, &named, changed_since_start);
        }
        Err(error) => {
            return Err(Unread::new(format!(
                "{} could not be read: {error}",
                named.display()
            )));
        }
    };

    if changed_since_start(file.changed_at) {
        return Err(Unread::new(format!(
            "{} changed after the node started, so it may be staged for the next restart rather \
             than what the node runs with",
            named.display()
        )));
    }
    let text = file.text;

    let documents = YamlLoader::load_from_str(&text)
        .map_err(|error| Unread::new(format!("{} is not YAML: {error}", named.display())))?;

    let mut values = BTreeMap::new();
    if let Some(document) = documents.first() {
        flatten(document, None, &mut values)
            .map_err(|reason| Unread::new(format!("{}: {reason}", named.display())))?;
    }

    values
        .into_iter()
        .map(|(name, value)| Ok((name, substitute(&value, environment)?)))
        .collect()
}

/// The settings a node with no file has from it: none, where its config directory is as it was
/// when the node started.
///
/// Found by the second domain review, measured on 8.15.3: a node starts and serves without the
/// file, configured by `-E` and its environment alone. What is suspect is a file gone since start,
/// and removing one changes the directory that held it, so that is what decides.
fn no_file_since_start(
    root: &Path,
    relative: &Path,
    named: &Path,
    changed_since_start: impl Fn(i64) -> bool,
) -> Result<BTreeMap<String, String>, Unread> {
    let directory = relative.parent().unwrap_or(Path::new(""));
    let changed_at = changed_at_inside(root, directory).map_err(|error| {
        Unread::new(format!(
            "{} is not there, and its directory could not be read: {error}",
            named.display()
        ))
    })?;

    match changed_since_start(changed_at) {
        true => Err(Unread::new(format!(
            "{} is not there, and its directory changed after the node started, so the file \
             may have been removed since and what the node read cannot be told",
            named.display()
        ))),
        false => Ok(BTreeMap::new()),
    }
}

/// Folds nested maps into dotted keys, the two spellings the node itself treats as one.
fn flatten(
    node: &Yaml,
    prefix: Option<&str>,
    values: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    match node {
        Yaml::Hash(entries) => {
            for (key, value) in entries {
                let key = scalar(key).ok_or("a key that is not a scalar")?;
                let name = match prefix {
                    Some(prefix) => format!("{prefix}.{key}"),
                    None => key,
                };
                flatten(value, Some(&name), values)?;
            }
            Ok(())
        }
        Yaml::Array(items) => {
            let name = prefix.ok_or("a list where the settings should be")?;
            let joined: Option<Vec<String>> = items.iter().map(scalar).collect();
            let joined =
                joined.ok_or_else(|| format!("{name} is a list of something other than values"))?;
            values.insert(name.to_owned(), joined.join(","));
            Ok(())
        }
        // An empty value leaves the setting on its default, as the node reads it.
        Yaml::Null => Ok(()),
        other => {
            let name = prefix.ok_or("a value where the settings should be")?;
            let value =
                scalar(other).ok_or_else(|| format!("{name} holds a value rastro cannot read"))?;
            values.insert(name.to_owned(), value);
            Ok(())
        }
    }
}

fn scalar(node: &Yaml) -> Option<String> {
    match node {
        Yaml::String(text) | Yaml::Real(text) => Some(text.clone()),
        Yaml::Integer(number) => Some(number.to_string()),
        Yaml::Boolean(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Resolves `${NAME}` and `${NAME:default}` from the node's own environment, as the node did at
/// start.
///
/// The default is used where the variable is unset, measured on 8.15.3: a node whose file said
/// `node.name: ${ES_UNSET_NAME:from-default}` started named `from-default`. A name the environment
/// does not hold and that gives no default is a refusal rather than the literal text: a port
/// spelled `${ES_HTTP_PORT}` is not one rastro may dial.
fn substitute(value: &str, environment: &BTreeMap<String, String>) -> Result<String, Unread> {
    let mut resolved = String::new();
    let mut rest = value;

    while let Some(start) = rest.find("${") {
        resolved.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| Unread::new(format!("`{value}` opens a variable it never closes")))?;
        let placeholder = &after[..end];
        let (name, default) = match placeholder.split_once(':') {
            Some((name, default)) => (name, Some(default)),
            None => (placeholder, None),
        };
        let found = environment
            .get(name)
            .map(String::as_str)
            .or(default)
            .ok_or_else(|| {
                Unread::new(format!(
                    "`{value}` names {name}, which the node's environment does not hold"
                ))
            })?;
        resolved.push_str(found);
        rest = &after[end + 1..];
    }

    resolved.push_str(rest);
    Ok(resolved)
}
