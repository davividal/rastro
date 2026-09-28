//! The settings a node was given at start: its argv, its environment and its own file.
//!
//! **Read only to decide whether and where the node may be asked**, which is the one reason
//! this collector parses a configuration file. The node's answer over its API is the
//! authoritative one, and the dispatch cannot have it yet: which port serves HTTP, and whether
//! that port wants TLS, have to be known before the first request, or the first request is a
//! guess. See `docs/decisions.md`.
//!
//! Three sources, in the precedence the node applies: a `-E` flag over an environment variable
//! named after the setting, over `elasticsearch.yml`. The docker image hands settings over as
//! environment variables whose names are the settings themselves, dots and all, and from 8.x
//! they appear nowhere in the argv, which is why the environment is read at all.
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
use crate::collectors::elasticsearch::value_objects::{Transport, Unread};

/// The argument vector's separator, and the environment's, which is how the kernel writes both.
const SEPARATOR: char = '\0';

/// The prefix a setting on the command line carries.
const COMMAND_LINE_SETTING: &str = "-E";

const CONFIG_FILE: &str = "elasticsearch.yml";

/// Where a node keeps its data, one directory or a comma-joined list of them.
const DATA_PATH: &str = "path.data";

/// Where a node keeps its data when nothing says, relative to its home.
const DEFAULT_DATA_DIRECTORY: &str = "data";

/// The setting that puts the HTTP listener behind TLS.
const TLS_SETTING: &str = "xpack.security.http.ssl.enabled";

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
        let config = node.config().ok_or_else(|| {
            Unread::new(
                "the node's argv names no es.path.conf, so its elasticsearch.yml cannot be \
                 located without guessing",
            )
        })?;

        let environment = read_pairs(&process.join("environ"), "environ")?;
        let arguments = read_list(&process.join("cmdline"), "cmdline")?;
        let file = read_config_file(&process.join("root"), config, &environment)?;

        let mut values = file;
        values.extend(
            environment
                .iter()
                .filter(|(name, _)| name.contains('.'))
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        values.extend(arguments.iter().filter_map(|argument| {
            let (name, value) = argument
                .strip_prefix(COMMAND_LINE_SETTING)?
                .split_once('=')?;
            Some((name.to_owned(), value.to_owned()))
        }));

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
}

/// Settings the caller already has, so what depends on them can be exercised without a node.
impl FromIterator<(String, String)> for NodeSettings {
    fn from_iter<Pairs: IntoIterator<Item = (String, String)>>(pairs: Pairs) -> Self {
        Self {
            values: pairs.into_iter().collect(),
        }
    }
}

fn read_list(path: &Path, what: &str) -> Result<Vec<String>, Unread> {
    let text = fs::read_to_string(path)
        .map_err(|error| Unread::new(format!("the node's {what} could not be read: {error}")))?;

    Ok(text
        .split(SEPARATOR)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect())
}

fn read_pairs(path: &Path, what: &str) -> Result<BTreeMap<String, String>, Unread> {
    Ok(read_list(path, what)?
        .iter()
        .filter_map(|entry| entry.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect())
}

/// The file's settings, or none where the directory holds no file.
///
/// A missing file is a node on its defaults and the other two sources, which is how it
/// started; a file that is there and cannot be read is a refusal.
fn read_config_file(
    root: &Path,
    config: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Unread> {
    let named = config.join(CONFIG_FILE);
    let under_root: PathBuf = root.join(named.strip_prefix("/").unwrap_or(&named));

    let text = match fs::read_to_string(&under_root) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(Unread::new(format!(
                "{} could not be read: {error}",
                named.display()
            )));
        }
    };

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

/// Resolves `${NAME}` from the node's own environment, which is what the node did at start.
///
/// A name the environment does not hold is a refusal rather than the literal text: a port
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
        let name = &after[..end];
        let found = environment.get(name).ok_or_else(|| {
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
