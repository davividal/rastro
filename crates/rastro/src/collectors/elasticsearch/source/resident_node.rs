//! The `/proc` interface: which Elasticsearch servers are running.
//!
//! **The gate every request hangs on.** rastro sends nothing to a listener whose holder it has
//! not identified as an Elasticsearch server, and this read is that identification. It asks
//! nothing of anything, so it cannot be the request that writes.
//!
//! Two argv shapes carry the same main class, measured on the official images: 7.17 is one
//! process with `org.elasticsearch.bootstrap.Elasticsearch` on the classpath, and 8.x and 9.x
//! are a `CliToolLauncher` parent forking a child that starts the class as a module,
//! `-m org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch`. The parent holds
//! no listener and is not a node, **but it holds the paths**: measured on 8.15.3, the server's
//! own argv carries no `-Des.path.*` and no `-E`, because the launcher hands them over a pipe.
//! So an 8.x server is read through its launcher's argv, found by the parent link in `stat`.
//!
//! A process id is a plain `u32` here rather than the `processes` facet's `ProcessId`, for the
//! reason the RabbitMQ residency read gives: that type is another collector's leaf value.

use std::fs;
use std::path::{Path, PathBuf};

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

/// The argument vector's separator, which is how the kernel writes `cmdline`.
const ARGUMENT_SEPARATOR: u8 = b'\0';

/// The main class as 7.x names it, a bare argument after `-cp`.
const CLASSPATH_MAIN: &str = "org.elasticsearch.bootstrap.Elasticsearch";

/// The main class as 8.x and 9.x name it, `module/class` after `-m`.
const MODULE_MAIN: &str = "org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch";

/// The program a server runs as, whichever JDK it is the `bin/java` of.
const JAVA: &str = "java";

/// The flags a JVM takes its classpath or its main module from.
const CLASSPATH_FLAGS: [&str; 3] = ["-cp", "-classpath", "--class-path"];
const MODULE_FLAGS: [&str; 2] = ["-m", "--module"];

/// The main class of the 8.x launcher, which forks the server.
const LAUNCHER_MAIN: &str = "org.elasticsearch.launcher.CliToolLauncher";

/// The system properties the launcher sets for where the node is installed and configured.
const HOME_PROPERTY: &str = "-Des.path.home=";
const CONFIG_PROPERTY: &str = "-Des.path.conf=";

/// A running Elasticsearch server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentNode {
    process_id: u32,

    /// The argv the node was launched with: its own on 7.x, its launcher's on 8.x and 9.x.
    launch_arguments: Vec<String>,

    /// Whether every launch argument was UTF-8, so the text above is the argv exactly.
    launch_arguments_are_exact: bool,

    /// `es.path.home`, where the node's `lib/` and `bin/` are.
    home: Option<PathBuf>,

    /// `es.path.conf`, the directory holding `elasticsearch.yml`.
    ///
    /// Absent where the argv does not carry it, which the official launchers always do: a
    /// node started by hand has no config directory this read can vouch for, and guessing
    /// `/etc/elasticsearch` would read a file that may belong to a different node.
    config: Option<PathBuf>,
}

impl ResidentNode {
    /// Reads the box's process table.
    pub fn all() -> Vec<Self> {
        Self::all_in(Path::new(PROC))
    }

    /// The same over a process table the caller names, in ascending process id order.
    ///
    /// **Never fails.** An unreadable `/proc` and an entry that vanished mid-walk both mean
    /// nothing was found to ask, and the caller's next step is to ask nothing.
    pub fn all_in(proc: &Path) -> Vec<Self> {
        let Ok(entries) = fs::read_dir(proc) else {
            return Vec::new();
        };

        let mut nodes: Vec<Self> = entries
            .flatten()
            .filter_map(|entry| Self::from_process_directory(proc, &entry.path()))
            .collect();

        // Directory order is the filesystem's, and a list that moves between two runs of an
        // unchanged box is what the document's contract forbids.
        nodes.sort_unstable_by_key(|node| node.process_id);
        nodes
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn config(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    /// Whether [`Self::launch_arguments`] is the argv exactly, rather than a lossy reading of
    /// an argument that was not UTF-8.
    ///
    /// The server is still a node either way, because the tokens that identify it are ASCII, and
    /// dropping it would be the silent absence this read must never produce. What a lossy argv
    /// cannot give is a setting or a path exactly as the node reads it.
    pub fn launch_arguments_are_exact(&self) -> bool {
        self.launch_arguments_are_exact
    }

    /// The argv the node was launched with, which is where its command-line settings are.
    pub fn launch_arguments(&self) -> &[String] {
        &self.launch_arguments
    }

    fn from_process_directory(proc: &Path, path: &Path) -> Option<Self> {
        let process_id: u32 = path.file_name()?.to_str()?.parse().ok()?;
        let own = arguments_of(path)?;
        let spelled: Vec<&str> = own.arguments.iter().map(String::as_str).collect();

        if !starts_the_server(&spelled) {
            return None;
        }

        // A launcher that exited, or a parent that is not one, lends nothing: taking any
        // parent's argv would read another program's flags as the node's settings.
        let launch = match starts_as_a_module(&spelled) {
            true => launcher_arguments(proc, path).unwrap_or(own),
            false => own,
        };
        let launched: Vec<&str> = launch.arguments.iter().map(String::as_str).collect();

        Some(Self {
            process_id,
            home: property_in(&launched, HOME_PROPERTY),
            config: property_in(&launched, CONFIG_PROPERTY),
            launch_arguments_are_exact: launch.exact,
            launch_arguments: launch.arguments,
        })
    }
}

/// A process's argv, each argument decoded on its own.
struct Argv {
    arguments: Vec<String>,

    /// False where any argument was not UTF-8 and was read lossily.
    exact: bool,
}

/// Read as bytes, because one argument that is not UTF-8, a Latin-1 path say, would otherwise
/// fail the whole read and the server would silently stop being a node.
fn arguments_of(process: &Path) -> Option<Argv> {
    let cmdline = fs::read(process.join("cmdline")).ok()?;
    let raw: Vec<&[u8]> = cmdline
        .split(|byte| *byte == ARGUMENT_SEPARATOR)
        .filter(|argument| !argument.is_empty())
        .collect();

    Some(Argv {
        exact: raw
            .iter()
            .all(|argument| std::str::from_utf8(argument).is_ok()),
        arguments: raw
            .iter()
            .map(|argument| String::from_utf8_lossy(argument).into_owned())
            .collect(),
    })
}

/// The argv of the process's parent, where the parent is the 8.x launcher.
///
/// The parent is the fourth field of `stat`, counted after the last `)`, because the second
/// field is the program name in parentheses and a name may hold spaces and parentheses itself.
fn launcher_arguments(proc: &Path, process: &Path) -> Option<Argv> {
    let stat = fs::read_to_string(process.join("stat")).ok()?;
    let parent = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?;
    let launcher = arguments_of(&proc.join(parent))?;
    let spelled: Vec<&str> = launcher.arguments.iter().map(String::as_str).collect();

    is_java_running(&spelled, LAUNCHER_MAIN).then_some(launcher)
}

/// Whether this argv is a JVM started with the server's main class.
///
/// **The class has to be the main class, not merely an argument.** Found by the conformance
/// run, whose own `pgrep -f org.elasticsearch.bootstrap.Elasticsearch` carries the class as a
/// whole argument and was read as a node. So the program must be `java`, and the class must be
/// what `-m` names or what follows the classpath `-cp` names, which is where a JVM takes it from.
fn starts_the_server(arguments: &[&str]) -> bool {
    starts_as_a_module(arguments) || is_java_running(arguments, CLASSPATH_MAIN)
}

/// Whether this is the 8.x and 9.x server, started as a module.
fn starts_as_a_module(arguments: &[&str]) -> bool {
    is_java(arguments)
        && arguments
            .windows(2)
            .any(|pair| MODULE_FLAGS.contains(&pair[0]) && pair[1] == MODULE_MAIN)
}

/// Whether this is a JVM whose main class, taken from the classpath, is `main`.
fn is_java_running(arguments: &[&str], main: &str) -> bool {
    is_java(arguments)
        && arguments
            .windows(3)
            .any(|triple| CLASSPATH_FLAGS.contains(&triple[0]) && triple[2] == main)
}

fn is_java(arguments: &[&str]) -> bool {
    arguments
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|program| program == JAVA)
}

/// The value of a `-D` system property, where the argv sets it.
fn property_in(arguments: &[&str], prefix: &str) -> Option<PathBuf> {
    arguments
        .iter()
        .find_map(|argument| argument.strip_prefix(prefix))
        .map(PathBuf::from)
}
