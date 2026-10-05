//! The `/proc` interface: which Elasticsearch servers are running.
//!
//! **The gate every request hangs on.** rastro sends nothing to a listener whose holder it has
//! not identified as an Elasticsearch server, and this read is that identification. It asks
//! nothing of anything, so it cannot be the request that writes.
//!
//! Two argv shapes carry the same main class, measured on the official images: 7.x is one
//! process with `org.elasticsearch.bootstrap.Elasticsearch` on the classpath, and 8.x and 9.x
//! start the class as a module, `-m org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch`,
//! forked by a launcher that holds no listener and is not a node.
//!
//! **Where the node is comes from the server process alone**, measured on every cell of the
//! matrix (`tests/fixtures/elasticsearch/cells`): the launcher differs per release (a Java
//! `CliToolLauncher` up to 9.2, a native `server-launcher` from 9.4) and exits once a node
//! started with `-d` is up, while the server always carries its install and its environment.
//! Its command-line settings are the exception: on 8.x and 9.x only the launcher's argv holds
//! them, so they are read from the parent while it is there.
//!
//! A process id is a plain `u32` here rather than the `processes` facet's `ProcessId`, for the
//! reason the RabbitMQ residency read gives: that type is another collector's leaf value.

use std::fs;
use std::path::{Path, PathBuf};

use crate::collectors::elasticsearch::source::ProcessOwner;
use crate::collectors::elasticsearch::source::in_root::{names_inside, read_inside};
use crate::collectors::elasticsearch::source::java_argument_file;
use crate::collectors::elasticsearch::value_objects::Release;

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

/// The main class of the Java launcher that forks an 8.x and 9.x server, up to 9.2.
const LAUNCHER_MAIN: &str = "org.elasticsearch.launcher.CliToolLauncher";

/// The main class of the 9.4 launcher where its native binary cannot run, measured in the
/// 9.4.7 `bin/elasticsearch`.
const SERVER_LAUNCHER_MAIN: &str = "org.elasticsearch.server.launcher.ServerLauncher";

/// The native launcher 9.4 and later fork the server from, `lib/tools/server-launcher/`. Not a
/// JVM, so its argv is its name and then the server's own options, `-E` among them.
const NATIVE_LAUNCHER: &str = "server-launcher";

/// The system properties the launcher sets for where the node is installed and configured.
const HOME_PROPERTY: &str = "-Des.path.home=";
const CONFIG_PROPERTY: &str = "-Des.path.conf=";

/// The option every 8.x and 9.x launcher names the server's module path with, its `lib/`.
const MODULE_PATH_OPTION: &str = "--module-path";

/// The environment variable the 8.x and 9.x launcher passes the config directory in.
const CONFIG_VARIABLE: &[u8] = b"ES_PATH_CONF=";

/// The config directory under the install, where neither the property nor the variable is set.
const DEFAULT_CONFIG: &str = "config";

/// The server jar's name around its version, `elasticsearch-<version>.jar` in `<home>/lib`.
const SERVER_JAR_PREFIX: &str = "elasticsearch-";
const SERVER_JAR_SUFFIX: &str = ".jar";

/// What the kernel appends to an open file's link once the file is removed.
const DELETED_MARKER: &str = " (deleted)";

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

    /// Where the node's `lib/` and `bin/` are: `es.path.home`, or on 8.15 and earlier 8.x, whose
    /// server is not given the property, the directory above its module path's `lib/`.
    home: Option<PathBuf>,

    /// The directory holding `elasticsearch.yml`: `es.path.conf` on 7.x, then the server's
    /// `ES_PATH_CONF`, then `<home>/config`, the order the launchers resolve it in.
    ///
    /// Absent where the environment cannot be read: the variable may be set there, a package
    /// install sets it to `/etc/elasticsearch`, and the default would name another directory.
    config: Option<PathBuf>,

    /// The release the node runs: the server jar it holds open, or where that cannot be seen, the
    /// one its install holds.
    release: Option<Release>,

    /// The release the node's install holds, read inside its own root, which an upgrade not yet
    /// followed by a restart makes another than the one it runs.
    installed_release: Option<Release>,

    /// When the process started, in clock ticks since boot, which with its id names this process
    /// and no later one given the same id.
    start: Option<u64>,

    /// The account the process runs as, which alone may be sent the operator's credential.
    owner: Option<ProcessOwner>,

    /// The first of the process's files a read needs that the kernel refused, where one was.
    refused: Option<&'static str>,

    /// `es.distribution.type`, which decides whether the environment holds settings at all.
    distribution: Option<String>,

    /// Whether a `java` argument file among the launch argv's options could not be read.
    launched_with_an_argument_file: bool,
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
    /// **Never fails.** An entry that vanished mid-walk is a process that left; an unreadable
    /// `/proc` is every process unseen, so the facet cannot call the box empty.
    pub fn census_in(proc: &Path) -> Census {
        let mut census = Census {
            nodes: Vec::new(),
            some_processes_unseen: false,
        };
        let Ok(entries) = fs::read_dir(proc) else {
            census.some_processes_unseen = true;
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

    /// When the process started, in clock ticks since boot, as the census found it.
    pub fn start(&self) -> Option<u64> {
        self.start
    }

    /// When the process with `process_id` started, now: field 22 of its `stat`, counted after the
    /// last `)`, since the name before it may hold spaces and parentheses.
    pub fn start_of_in(proc: &Path, process_id: u32) -> Option<u64> {
        let stat = fs::read_to_string(proc.join(process_id.to_string()).join("stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }

    /// Which release the node runs, as the server jar it holds open names it.
    ///
    /// Not `GET /`, which a node with security on refuses: the version decides how a node is
    /// read before it is asked. Found by the third domain review, measured on cell 31: a package
    /// upgraded under a running node replaces the jar in `lib/` and leaves the node running the
    /// old one, held open and marked ` (deleted)`, so the install is the fallback, where the
    /// descriptors cannot be listed. Nothing where neither names exactly one server jar.
    pub fn release(&self) -> Option<Release> {
        self.release
    }

    /// The first of `environ`, `fd` and `root` under the node's `/proc` entry that was refused.
    ///
    /// Found by the third domain review, measured as `nobody`: another account's are refused to
    /// an unprivileged run, and the node read as an error blaming a missing jar. A file that is
    /// not there is not refused: it is a process that has nothing there.
    pub fn refused(&self) -> Option<&'static str> {
        self.refused
    }

    /// Which release the node's install holds, as its `lib/` names it.
    pub fn installed_release(&self) -> Option<Release> {
        self.installed_release
    }

    /// Whether [`Self::application_arguments`], and the paths before them, are the argv exactly,
    /// rather than a lossy reading of an argument that was not UTF-8.
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

    /// Whether the node was launched with a `java` argument file, `@file`, that could not be read.
    ///
    /// The launcher expands one in place, and a readable one is expanded here the same way. One
    /// that cannot be read may hold a property, a later `es.path.conf` say, that overrides what the
    /// argv shows, so a node launched with one cannot have its paths read as the JVM read them.
    pub fn launched_with_an_argument_file(&self) -> bool {
        self.launched_with_an_argument_file
    }

    /// The account the process runs as, read between the two looks at its start.
    pub fn owner(&self) -> Option<&ProcessOwner> {
        self.owner.as_ref()
    }

    /// The arguments after the launch argv's entry point, which is where the command-line
    /// settings are.
    pub fn application_arguments(&self) -> &[String] {
        &self.application_arguments
    }

    fn inspect(proc: &Path, path: &Path) -> Inspection {
        let Some(process_id) = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|name| name.parse::<u32>().ok())
        else {
            return Inspection::NotANode;
        };
        // Found by review: the start is taken before anything else is read and again after, so a
        // process id reused mid-read cannot lend the old node's identity to the new process.
        let start = Self::start_of_in(proc, process_id);
        let node = match arguments_of(path) {
            Ok(own) if may_hide_the_server(&own) => return Inspection::Unseen,
            Ok(own) => match Self::from_arguments(proc, path, process_id, start, own) {
                Some(node) => node,
                None => return Inspection::NotANode,
            },
            Err(error) if has_left(&error) => return Inspection::Left,
            Err(_) => return Inspection::Unseen,
        };

        match Self::start_of_in(proc, process_id) == start {
            true => Inspection::Node(Box::new(node)),
            false => Inspection::Left,
        }
    }

    fn from_arguments(
        proc: &Path,
        path: &Path,
        process_id: u32,
        start: Option<u64>,
        own: Argv,
    ) -> Option<Self> {
        let spelled: Vec<&str> = own.arguments.iter().map(String::as_str).collect();

        if !starts_the_server(&spelled) {
            return None;
        }

        let home = property_in(&spelled, HOME_PROPERTY).or_else(|| module_install(&spelled));
        let config = property_in(&spelled, CONFIG_PROPERTY)
            .or_else(|| configured_in_environment(path, home.as_deref()));
        let distribution = property_in(&spelled, DISTRIBUTION_PROPERTY)
            .map(|distribution| distribution.to_string_lossy().into_owned());
        let installed_release = home
            .as_deref()
            .and_then(|home| installed_release(path, home));
        let release = running_release(path).or(installed_release);
        let module_server = starts_as_a_module(&spelled);

        // A launcher that exited, or a parent that is not one, lends nothing: taking any
        // parent's argv would read another program's flags as the node's settings.
        let launch = match module_server {
            true => launcher_arguments(proc, path).unwrap_or(own),
            false => own,
        };
        let launched: Vec<&str> = launch.arguments.iter().map(String::as_str).collect();

        Some(Self {
            process_id,
            start,
            owner: ProcessOwner::of_in(proc, process_id),
            refused: refused_in(path),
            home,
            config,
            release,
            installed_release,
            distribution,
            launched_with_an_argument_file: launch.unread_argument_file,
            launch_arguments_are_exact: launch.exact,
            application_arguments: application_start(&launched)
                .map(|start| launch.arguments.get(start..).unwrap_or_default().to_vec())
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

/// Whether a read failed because the process exited, as opposed to being refused.
fn has_left(error: &std::io::Error) -> bool {
    /// `ESRCH`, which a read of a process that exited mid-read can return.
    const NO_SUCH_PROCESS: i32 = 3;

    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(NO_SUCH_PROCESS)
}

/// Read as bytes, because one argument that is not UTF-8, a Latin-1 path say, would otherwise
/// fail the whole read and the server would silently stop being a node.
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
    // Found by the security review: every process is inspected, as root, and `curl -d @fifo`
    // blocked the census on the FIFO. The `@` is only java's to expand.
    let launched_by_java = is_java(
        &expansion
            .arguments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );

    for argument in rest {
        let expanding = launched_by_java && scan.expanding();
        let from_argument: Vec<String> = match (expanding, argument.strip_prefix('@')) {
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
    read_inside(&process.join("root"), relative).ok()
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

/// The first of the process's files a read needs that the kernel refused.
fn refused_in(process: &Path) -> Option<&'static str> {
    let refused = |outcome: std::io::Result<()>| {
        outcome.is_err_and(|error| error.kind() != std::io::ErrorKind::NotFound)
    };

    if refused(fs::read(process.join("environ")).map(drop)) {
        return Some("environ");
    }
    if refused(fs::read_dir(process.join("fd")).map(drop)) {
        return Some("fd");
    }
    if refused(fs::read_dir(process.join("root")).map(drop)) {
        return Some("root");
    }
    None
}

/// Where a launch argv's own arguments begin: after the entry point of a JVM, after the name of
/// the native launcher.
fn application_start(arguments: &[&str]) -> Option<usize> {
    match is_native_launcher(arguments) {
        true => Some(1),
        false => launch_of(arguments).map(|launch| launch.arguments_from),
    }
}

fn is_native_launcher(arguments: &[&str]) -> bool {
    arguments
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|program| program == NATIVE_LAUNCHER)
}

/// The argv of the process's parent, where the parent is a launcher that forks the server.
///
/// The parent is the fourth field of `stat`, counted after the last `)`, because the second
/// field is the program name in parentheses and a name may hold spaces and parentheses itself.
fn launcher_arguments(proc: &Path, process: &Path) -> Option<Argv> {
    let stat = fs::read_to_string(process.join("stat")).ok()?;
    let parent = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?;
    let launcher = arguments_of(&proc.join(parent)).ok()?;
    let spelled: Vec<&str> = launcher.arguments.iter().map(String::as_str).collect();

    let launches_the_server = is_native_launcher(&spelled)
        || is_java_running(&spelled, LAUNCHER_MAIN)
        || is_java_running(&spelled, SERVER_LAUNCHER_MAIN);

    launches_the_server.then_some(launcher)
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

/// Whether a `java` that does not start the server could still be one: an argument file it
/// could not read stood among its options, and may hold the main class.
///
/// Found by review: dropped, such a node left the facet and its directories unsealed.
fn may_hide_the_server(launch: &Argv) -> bool {
    let arguments: Vec<&str> = launch.arguments.iter().map(String::as_str).collect();
    launch.unread_argument_file && is_java(&arguments) && !starts_the_server(&arguments)
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

/// The install a module-path server was started from: the directory above the `lib/` its
/// module path names, `--module-path <home>/lib` on every launcher measured.
fn module_install(arguments: &[&str]) -> Option<PathBuf> {
    let options = &arguments[..launch_of(arguments)?.entry_index];
    let module_path = options
        .windows(2)
        .rev()
        .find(|pair| pair[0] == MODULE_PATH_OPTION)?[1];
    let lib = Path::new(module_path);

    (lib.file_name()? == "lib").then(|| lib.parent().map(Path::to_path_buf))?
}

/// The server jar's version among the install's `lib/`, read inside the process's root.
fn installed_release(process: &Path, home: &Path) -> Option<Release> {
    let lib = home.join("lib");
    let names = names_inside(&process.join("root"), lib.strip_prefix("/").ok()?).ok()?;
    single_release(names.iter().map(String::as_str))
}

/// The server jar's version among the files the process holds open, ` (deleted)` taken off.
fn running_release(process: &Path) -> Option<Release> {
    let targets: Vec<String> = fs::read_dir(process.join("fd"))
        .ok()?
        .flatten()
        .filter_map(|descriptor| fs::read_link(descriptor.path()).ok())
        .filter_map(|target| target.to_str().map(str::to_owned))
        .collect();
    let names = targets.iter().filter_map(|target| {
        let path = target.strip_suffix(DELETED_MARKER).unwrap_or(target);
        Path::new(path).file_name()?.to_str()
    });

    single_release(names)
}

/// The one release the server jars among `names` name, where they name exactly one. The other
/// jars are named `elasticsearch-<module>-<version>.jar`, which no version parses.
fn single_release<'name>(names: impl Iterator<Item = &'name str>) -> Option<Release> {
    let mut versions: Vec<Release> = names
        .filter_map(|name| {
            name.strip_prefix(SERVER_JAR_PREFIX)?
                .strip_suffix(SERVER_JAR_SUFFIX)
                .and_then(Release::parse)
        })
        .collect();
    versions.sort();
    versions.dedup();

    match versions.as_slice() {
        [release] => Some(*release),
        _ => None,
    }
}

/// The config directory as the server's environment gives it: `ES_PATH_CONF`, else the default
/// under the install. Nothing where the environment cannot be read or names a relative path,
/// which would resolve against a working directory this read does not know the node had.
fn configured_in_environment(process: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let environment = fs::read(process.join("environ")).ok()?;
    let variable = environment
        .split(|byte| *byte == ARGUMENT_SEPARATOR)
        .rev()
        .find_map(|entry| entry.strip_prefix(CONFIG_VARIABLE));

    match variable {
        Some(value) => {
            let directory = PathBuf::from(std::str::from_utf8(value).ok()?);
            directory.is_absolute().then_some(directory)
        }
        None => home.map(|home| home.join(DEFAULT_CONFIG)),
    }
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
