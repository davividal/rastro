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

use crate::collectors::elasticsearch::source::in_root::read_inside;
use crate::collectors::elasticsearch::source::java_argument_file;

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

    /// The arguments after the entry point of the argv the node was launched with: the server's
    /// own on 7.x, its launcher's on 8.x and 9.x. Its command-line settings are among them.
    application_arguments: Vec<String>,

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

    /// Whether a `java` argument file among the launch argv's options could not be read.
    launched_with_an_argument_file: bool,

    /// When the server process started, in seconds since the epoch.
    started_at: Option<u64>,

    /// Whether this is an 8.x or 9.x server whose parent is not its launcher.
    launcher_gone: bool,
}

/// The servers on a process table, and whether any process could not be inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// In ascending process id order.
    pub nodes: Vec<ResidentNode>,

    /// Whether some process's argv was refused rather than gone, so a node could be among them.
    ///
    /// Found by the second domain review: under `hidepid=1` an unprivileged run sees a process's
    /// directory and is refused its `cmdline`, and that was read as a process that had exited,
    /// so a node hidden that way left the facet `absent`. `hidepid=2` hides the directory itself,
    /// which no reading can tell from no process.
    pub some_processes_unseen: bool,
}

/// What one process table entry turned out to be.
enum Inspection {
    Node(Box<ResidentNode>),
    NotANode,

    /// It exited between the listing and the read.
    Left,

    /// Its argv was refused, for a reason other than having exited.
    Unseen,
}

impl ResidentNode {
    /// The servers on a process table the caller names, and whether any process was unseen.
    ///
    /// **Never fails.** An unreadable `/proc` and an entry that vanished mid-walk both mean
    /// nothing was found to ask.
    pub fn census_in(proc: &Path) -> Census {
        let mut census = Census {
            nodes: Vec::new(),
            some_processes_unseen: false,
        };
        let Ok(entries) = fs::read_dir(proc) else {
            return census;
        };

        for entry in entries.flatten() {
            match Self::inspect(proc, &entry.path()) {
                Inspection::Node(node) => census.nodes.push(*node),
                Inspection::Unseen => census.some_processes_unseen = true,
                Inspection::NotANode | Inspection::Left => {}
            }
        }

        // Directory order is the filesystem's, and a list that moves between two runs of an
        // unchanged box is what the document's contract forbids.
        census.nodes.sort_unstable_by_key(|node| node.process_id);
        census
    }

    /// Reads the box's process table.
    pub fn all() -> Vec<Self> {
        Self::all_in(Path::new(PROC))
    }

    /// The servers on a process table the caller names, in ascending process id order.
    pub fn all_in(proc: &Path) -> Vec<Self> {
        Self::census_in(proc).nodes
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

    /// Whether this is an 8.x or 9.x server whose launcher is no longer its parent.
    ///
    /// Found by the second domain review, measured on 8.15.3: `bin/elasticsearch -d` returns once
    /// the node is up and its launcher exits, so the server is reparented, and the paths and
    /// settings the launcher passed it over a pipe are nowhere on the box.
    pub fn launcher_gone(&self) -> bool {
        self.launcher_gone
    }

    /// When the server process started, in seconds since the epoch, where `/proc` says.
    ///
    /// What the node's file is compared against, since the file the node read is the one it
    /// had at start: see [`NodeSettings`](super::NodeSettings).
    pub fn started_at(&self) -> Option<u64> {
        self.started_at
    }

    /// Whether the node was launched with a `java` argument file, `@file`, that could not be read.
    ///
    /// The launcher expands one in place, and a readable one is expanded here the same way. One
    /// that cannot be read may hold a property, a later `es.path.conf` say, that overrides what the
    /// argv shows, so a node launched with one cannot have its paths read as the JVM read them.
    pub fn launched_with_an_argument_file(&self) -> bool {
        self.launched_with_an_argument_file
    }

    /// The arguments after the launch argv's entry point, which is where the command-line
    /// settings are.
    pub fn application_arguments(&self) -> &[String] {
        &self.application_arguments
    }

    fn inspect(proc: &Path, path: &Path) -> Inspection {
        let Some(process_id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
        else {
            return Inspection::NotANode;
        };
        match arguments_of(path) {
            Ok(own) => Self::from_arguments(proc, path, process_id, own)
                .map_or(Inspection::NotANode, |node| {
                    Inspection::Node(Box::new(node))
                }),
            Err(error) if has_left(&error) => Inspection::Left,
            Err(_) => Inspection::Unseen,
        }
    }

    fn from_arguments(proc: &Path, path: &Path, process_id: u32, own: Argv) -> Option<Self> {
        let spelled: Vec<&str> = own.arguments.iter().map(String::as_str).collect();

        if !starts_the_server(&spelled) {
            return None;
        }

        // A launcher that exited, or a parent that is not one, lends nothing: taking any
        // parent's argv would read another program's flags as the node's settings.
        let (launch, launcher_gone) = match starts_as_a_module(&spelled) {
            true => match launcher_arguments(proc, path) {
                Some(launcher) => (launcher, false),
                None => (own, true),
            },
            false => (own, false),
        };
        let launched: Vec<&str> = launch.arguments.iter().map(String::as_str).collect();

        Some(Self {
            process_id,
            home: property_in(&launched, HOME_PROPERTY),
            config: property_in(&launched, CONFIG_PROPERTY),
            distribution: property_in(&launched, DISTRIBUTION_PROPERTY)
                .map(|distribution| distribution.to_string_lossy().into_owned()),
            started_at: started_at(proc, path),
            launcher_gone,
            launched_with_an_argument_file: launch.unread_argument_file,
            launch_arguments_are_exact: launch.exact,
            application_arguments: launch_of(&launched)
                .map(|start| {
                    launch
                        .arguments
                        .get(start.arguments_from..)
                        .unwrap_or_default()
                        .to_vec()
                })
                .unwrap_or_default(),
        })
    }
}

/// A process's argv, each argument decoded on its own, its argument files expanded.
struct Argv {
    arguments: Vec<String>,

    /// False where any argument was not UTF-8 and was read lossily.
    exact: bool,

    /// Whether an argument file among the launcher's options could not be read, so the argv
    /// may hold options, a path among them, that `/proc` does not show.
    unread_argument_file: bool,
}

/// Read as bytes, because one argument that is not UTF-8, a Latin-1 path say, would otherwise
/// fail the whole read and the server would silently stop being a node.
/// Whether a read failed because the process exited, as opposed to being refused.
fn has_left(error: &std::io::Error) -> bool {
    /// `ESRCH`, which a read of a process that exited mid-read can return.
    const NO_SUCH_PROCESS: i32 = 3;

    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(NO_SUCH_PROCESS)
}

fn arguments_of(process: &Path) -> std::io::Result<Argv> {
    let cmdline = fs::read(process.join("cmdline"))?;
    let raw: Vec<&[u8]> = cmdline
        .split(|byte| *byte == ARGUMENT_SEPARATOR)
        .filter(|argument| !argument.is_empty())
        .collect();

    let arguments: Vec<String> = raw
        .iter()
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect();
    let expansion = expanded(process, arguments);

    Ok(Argv {
        exact: raw
            .iter()
            .all(|argument| std::str::from_utf8(argument).is_ok()),
        arguments: expansion.arguments,
        unread_argument_file: expansion.unread_argument_file,
    })
}

/// An argv with its argument files expanded in place.
struct Expansion {
    arguments: Vec<String>,
    unread_argument_file: bool,
}

/// The argv as the `java` launcher sees it: each `@file` among the options replaced by the
/// arguments it holds, read inside the process's own root and relative to its working directory.
///
/// Found by review: `java @args` may carry the whole launch in the file, main class included,
/// and read only as the argv the server silently stopped being a node. As the launcher does,
/// measured on the bundled JDK: `@@name` is the argument `@name`, `--disable-@files` stops the
/// expansion, and nothing after the entry point is expanded, being the application's own.
fn expanded(process: &Path, arguments: Vec<String>) -> Expansion {
    let mut expansion = Expansion {
        arguments: Vec::with_capacity(arguments.len()),
        unread_argument_file: false,
    };
    let mut scan = OptionScan::default();
    let mut rest = arguments.into_iter();
    expansion.arguments.extend(rest.next());

    for argument in rest {
        let from_argument: Vec<String> = match (scan.expanding(), argument.strip_prefix('@')) {
            (true, Some(literal)) if literal.starts_with('@') => vec![literal.to_owned()],
            (true, Some(file)) => match argument_file_text(process, file) {
                Some(text) => java_argument_file::arguments_in(&text),
                // Dropped rather than kept: kept, `@file` reads as the main class and the server
                // silently stops being a node, the defect this expansion exists to close.
                None => {
                    expansion.unread_argument_file = true;
                    Vec::new()
                }
            },
            _ => vec![argument],
        };
        for token in from_argument {
            scan.feed(&token);
            expansion.arguments.push(token);
        }
    }

    expansion
}

/// An argument file's text, resolved inside the process's root from its working directory.
fn argument_file_text(process: &Path, file: &str) -> Option<String> {
    let named = Path::new(file);
    let absolute = match named.is_absolute() {
        true => named.to_path_buf(),
        false => fs::read_link(process.join("cwd")).ok()?.join(named),
    };
    let relative = absolute.strip_prefix("/").ok()?;
    read_inside(&process.join("root"), relative)
        .ok()
        .map(|file| file.text)
}

/// Where the launcher's option scan is, fed one argument at a time.
#[derive(Debug, Default)]
struct OptionScan {
    /// The next argument is the value of the option before it.
    awaiting_value: bool,

    /// The next argument is the entry point, after `-m` or `-jar`.
    awaiting_entry: bool,

    /// The entry point has been read, so what follows is the application's.
    done: bool,

    /// `--disable-@files` was seen.
    disabled: bool,
}

impl OptionScan {
    fn expanding(&self) -> bool {
        !self.done && !self.disabled
    }

    fn feed(&mut self, argument: &str) {
        if self.done {
            return;
        }
        if self.awaiting_entry {
            self.done = true;
        } else if self.awaiting_value {
            self.awaiting_value = false;
        } else if argument == "--disable-@files" {
            self.disabled = true;
        } else if MODULE_FLAGS.contains(&argument) || argument == JAR_OPTION {
            self.awaiting_entry = true;
        } else if argument.starts_with("--module=") {
            self.done = true;
        } else if OPTIONS_WITH_A_VALUE.contains(&argument) {
            self.awaiting_value = true;
        } else if !argument.starts_with('-') {
            self.done = true;
        }
    }
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
    let launcher = arguments_of(&proc.join(parent)).ok()?;
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

    /// The index of the first argument after the entry point, the application's own.
    arguments_from: usize,
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

    let mut index = 1;
    while let Some(argument) = arguments.get(index) {
        let launch = |entry, entry_index, arguments_from| Launch {
            entry,
            entry_index,
            arguments_from,
        };

        if MODULE_FLAGS.contains(argument) {
            return arguments
                .get(index + 1)
                .map(|module| launch(EntryPoint::Module(module), index, index + 2));
        }
        if let Some(module) = argument.strip_prefix("--module=") {
            return Some(launch(EntryPoint::Module(module), index, index + 1));
        }
        if *argument == JAR_OPTION {
            return Some(launch(EntryPoint::Jar, index, index + 2));
        }
        if OPTIONS_WITH_A_VALUE.contains(argument) {
            index += 2;
            continue;
        }
        if !argument.starts_with('-') {
            return Some(launch(EntryPoint::Class(argument), index, index + 1));
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
