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
//! no listener and is not a node.
//!
//! A process id is a plain `u32` here rather than the `processes` facet's `ProcessId`, for the
//! reason the RabbitMQ residency read gives: that type is another collector's leaf value.

use std::fs;
use std::path::{Path, PathBuf};

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

/// The argument vector's separator, which is how the kernel writes `cmdline`.
const ARGUMENT_SEPARATOR: char = '\0';

/// The main class as 7.x names it, a bare argument after `-cp`.
const CLASSPATH_MAIN: &str = "org.elasticsearch.bootstrap.Elasticsearch";

/// The main class as 8.x and 9.x name it, `module/class` after `-m`.
const MODULE_MAIN: &str = "org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch";

/// The program a server runs as, whichever JDK it is the `bin/java` of.
const JAVA: &str = "java";

/// The flags a JVM takes its classpath or its main module from.
const CLASSPATH_FLAGS: [&str; 3] = ["-cp", "-classpath", "--class-path"];
const MODULE_FLAGS: [&str; 2] = ["-m", "--module"];

/// The system properties the launcher sets for where the node is installed and configured.
const HOME_PROPERTY: &str = "-Des.path.home=";
const CONFIG_PROPERTY: &str = "-Des.path.conf=";

/// A running Elasticsearch server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentNode {
    process_id: u32,

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
            .filter_map(|entry| Self::from_process_directory(&entry.path()))
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

    fn from_process_directory(path: &Path) -> Option<Self> {
        let process_id: u32 = path.file_name()?.to_str()?.parse().ok()?;
        let cmdline = fs::read_to_string(path.join("cmdline")).ok()?;
        let arguments: Vec<&str> = cmdline
            .split(ARGUMENT_SEPARATOR)
            .filter(|argument| !argument.is_empty())
            .collect();

        if !starts_the_server(&arguments) {
            return None;
        }

        Some(Self {
            process_id,
            home: property_in(&arguments, HOME_PROPERTY),
            config: property_in(&arguments, CONFIG_PROPERTY),
        })
    }
}

/// Whether this argv is a JVM started with the server's main class.
///
/// **The class has to be the main class, not merely an argument.** Found by the conformance
/// run, whose own `pgrep -f org.elasticsearch.bootstrap.Elasticsearch` carries the class as a
/// whole argument and was read as a node. So the program must be `java`, and the class must be
/// what `-m` names or what follows the classpath `-cp` names, which is where a JVM takes it from.
fn starts_the_server(arguments: &[&str]) -> bool {
    let is_java = arguments
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|program| program == JAVA);

    let module_main = arguments
        .windows(2)
        .any(|pair| MODULE_FLAGS.contains(&pair[0]) && pair[1] == MODULE_MAIN);
    let classpath_main = arguments
        .windows(3)
        .any(|triple| CLASSPATH_FLAGS.contains(&triple[0]) && triple[2] == CLASSPATH_MAIN);

    is_java && (module_main || classpath_main)
}

/// The value of a `-D` system property, where the argv sets it.
fn property_in(arguments: &[&str], prefix: &str) -> Option<PathBuf> {
    arguments
        .iter()
        .find_map(|argument| argument.strip_prefix(prefix))
        .map(PathBuf::from)
}
