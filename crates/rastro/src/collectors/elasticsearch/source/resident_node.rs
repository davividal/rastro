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
const MODULE_FLAGS: [&str; 2] = ["-m", "--module"];

/// The `java` launcher options whose value is the next argument rather than part of their own.
///
/// What lets the main class be found: it is the first argument that is neither an option nor
/// one of these values. Options spelled `--name=value` are one argument and need no entry.
const OPTIONS_WITH_A_VALUE: [&str; 13] = [
    "-cp",
    "-classpath",
    "--class-path",
    "-p",
    "--module-path",
    "--upgrade-module-path",
    "--add-modules",
    "--add-reads",
    "--add-exports",
    "--add-opens",
    "--limit-modules",
    "--patch-module",
    "--enable-native-access",
];

/// The option that makes a jar's manifest name the main class instead.
const JAR_OPTION: &str = "-jar";

/// The main class of the 8.x launcher, which forks the server.
const LAUNCHER_MAIN: &str = "org.elasticsearch.launcher.CliToolLauncher";

/// The system properties the launcher sets for where the node is installed and configured.
const HOME_PROPERTY: &str = "-Des.path.home=";
const CONFIG_PROPERTY: &str = "-Des.path.conf=";

/// The system property naming how the node was installed: `docker`, `tar`, `deb` or `rpm`.
const DISTRIBUTION_PROPERTY: &str = "-Des.distribution.type=";

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

    /// `es.distribution.type`, which decides whether the environment holds settings at all.
    distribution: Option<String>,

    /// Whether the launch argv names a `java` argument file among its options.
    launched_with_an_argument_file: bool,

    /// When the server process started, in seconds since the epoch.
    started_at: Option<u64>,
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

    /// How the node was installed, as its launch argv names it.
    pub fn distribution(&self) -> Option<&str> {
        self.distribution.as_deref()
    }

    /// When the server process started, in seconds since the epoch, where `/proc` says.
    ///
    /// What the node's file is compared against, since the file the node read is the one it
    /// had at start: see [`NodeSettings`](super::NodeSettings).
    pub fn started_at(&self) -> Option<u64> {
        self.started_at
    }

    /// Whether the node was launched with a `java` argument file, `@file`, among its options.
    ///
    /// The launcher expands one in place, so a property in it, a later `es.path.conf` say,
    /// overrides what the argv shows, and its content is not in `/proc`. A node launched this way
    /// cannot have its paths read as the JVM read them.
    pub fn launched_with_an_argument_file(&self) -> bool {
        self.launched_with_an_argument_file
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
            distribution: property_in(&launched, DISTRIBUTION_PROPERTY)
                .map(|distribution| distribution.to_string_lossy().into_owned()),
            started_at: started_at(proc, path),
            launched_with_an_argument_file: launch_of(&launched)
                .is_some_and(|launch| launch.argument_file),
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

/// When a process started: the boot time from `/proc/stat` plus field 22 of its own `stat`, which
/// counts clock ticks since boot. Counted after the last `)`, as the parent is, because the name
/// before it may hold spaces.
fn started_at(proc: &Path, process: &Path) -> Option<u64> {
    let stat = fs::read_to_string(process.join("stat")).ok()?;
    let ticks: u64 = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()?;
    let boot: u64 = fs::read_to_string(proc.join("stat"))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;

    Some(boot + ticks / clock_ticks_per_second())
}

#[cfg(target_os = "linux")]
fn clock_ticks_per_second() -> u64 {
    rustix::param::clock_ticks_per_second()
}

/// The Linux default, for a workstation build that reads no real node.
#[cfg(not(target_os = "linux"))]
fn clock_ticks_per_second() -> u64 {
    100
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
/// whole argument and was read as a node; and by review, since everything after a JVM's main
/// class is that application's own argument, a `-cp` and the server's class among them
/// included. So the program must be `java`, and its entry point, read from the launcher's
/// options and no further, must be the server's.
fn starts_the_server(arguments: &[&str]) -> bool {
    starts_as_a_module(arguments) || is_java_running(arguments, CLASSPATH_MAIN)
}

/// Whether this is the 8.x and 9.x server, started as a module.
fn starts_as_a_module(arguments: &[&str]) -> bool {
    launch_of(arguments).is_some_and(|launch| launch.entry == EntryPoint::Module(MODULE_MAIN))
}

/// Whether this is a JVM whose main class, taken from the classpath, is `main`.
fn is_java_running(arguments: &[&str], main: &str) -> bool {
    launch_of(arguments).is_some_and(|launch| launch.entry == EntryPoint::Class(main))
}

/// How a `java` argv launched: its entry point, where that is, and what its options hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Launch<'argv> {
    entry: EntryPoint<'argv>,

    /// The index of the argument the entry point was read from; every option is before it.
    entry_index: usize,

    /// Whether an `@file` came among the options.
    argument_file: bool,
}

/// What a JVM was started to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryPoint<'argv> {
    /// A main class, from the classpath.
    Class(&'argv str),

    /// A `module/class`, after `-m`.
    Module(&'argv str),

    /// A jar, whose manifest names the class.
    Jar,
}

/// The launch of a `java` argv: the launcher's options are skipped, with the values of the ones
/// that take one, and the first thing that is not an option is the entry point. Nothing after it
/// is read.
fn launch_of<'argv>(arguments: &[&'argv str]) -> Option<Launch<'argv>> {
    if !is_java(arguments) {
        return None;
    }

    let mut argument_file = false;
    let mut index = 1;
    while let Some(argument) = arguments.get(index) {
        let launch = |entry, entry_index| Launch {
            entry,
            entry_index,
            argument_file,
        };

        if MODULE_FLAGS.contains(argument) {
            return arguments
                .get(index + 1)
                .map(|module| launch(EntryPoint::Module(module), index));
        }
        if let Some(module) = argument.strip_prefix("--module=") {
            return Some(launch(EntryPoint::Module(module), index));
        }
        if *argument == JAR_OPTION {
            return Some(launch(EntryPoint::Jar, index));
        }
        if OPTIONS_WITH_A_VALUE.contains(argument) {
            index += 2;
            continue;
        }
        if argument.starts_with('@') {
            argument_file = true;
        } else if !argument.starts_with('-') {
            return Some(launch(EntryPoint::Class(argument), index));
        }
        index += 1;
    }

    None
}

fn is_java(arguments: &[&str]) -> bool {
    arguments
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|program| program == JAVA)
}

/// The value of a `-D` system property, where the argv sets it: the last one, as the JVM takes.
///
/// Found by review, and confirmed there on OpenJDK 11 to 25: `-Done=first -Done=second` sets
/// `second`. Only the launcher's options are read, since a `-D` after the entry point is the
/// application's argument rather than the JVM's.
fn property_in(arguments: &[&str], prefix: &str) -> Option<PathBuf> {
    let options = launch_of(arguments).map_or(arguments.len(), |launch| launch.entry_index);
    arguments[..options]
        .iter()
        .rev()
        .find_map(|argument| argument.strip_prefix(prefix))
        .map(PathBuf::from)
}
