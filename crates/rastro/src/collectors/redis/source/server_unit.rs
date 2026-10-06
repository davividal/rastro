//! Which systemd unit started a server, and what it started it with.
//!
//! **Only asked when a server has refused to answer without a password.** The unit's start
//! command is where the configuration file is named, and the configuration file is where the
//! password is; a server that answers freely needs neither and systemd is not bothered.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::redis::value_objects::ServerKind;
use crate::collectors::systemd::systemctl_show;

/// The suffix of the one kind of unit that runs a daemon.
const SERVICE: &str = ".service";

/// What `systemctl` is asked: the command a unit starts, resolved through every drop-in.
const SHOW: [&str; 4] = [
    "show",
    "--property=Id",
    "--property=ExecStartEx",
    "--no-pager",
];

/// What separates options from the unit name, so a name can never be read as an option.
const END_OF_OPTIONS: &str = "--";

/// The option that sets the password on redis's command line.
const REQUIREPASS_OPTION: &str = "--requirepass";

/// How a server was started, as far as finding its password needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerStart {
    /// The configuration file, where the start command names one.
    pub config_file: Option<PathBuf>,

    /// A password given on the command line, which redis applies after the file.
    pub password: Option<String>,
}

/// The service unit a process's cgroup names, and the cgroup itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCgroup {
    pub unit: String,

    /// The process's cgroup path, `/system.slice/redis-server.service`, which systemd's own
    /// `ControlGroup` for the unit has to match before the unit is believed.
    pub path: String,
}

/// The service unit a process runs in, from its cgroup.
///
/// **From the kernel's record rather than by asking systemd which unit owns a pid**, because the
/// cgroup file is readable by anybody and asking systemd is a question per process. Only the last
/// component is the unit: a template instance nests under its template's slice,
/// `system-redis\x2dserver.slice/redis-server@cache.service`.
///
/// **Refused unless it is a plain service name**, since it becomes an argument to `systemctl`
/// and every other argument rastro passes is a literal an author wrote.
pub fn unit_of(proc: &Path, process_id: u32) -> Result<ServerCgroup, String> {
    let cgroup = fs::read_to_string(proc.join(process_id.to_string()).join("cgroup"))
        .map_err(|error| format!("its cgroup could not be read ({error})"))?;

    // `hierarchy:controllers:path`, and the path may hold a colon of its own.
    let (unit, path) = cgroup
        .lines()
        .filter_map(|line| line.splitn(3, ':').nth(2))
        .find_map(|path| {
            let unit = path.rsplit('/').next()?;
            unit.ends_with(SERVICE).then_some((unit, path))
        })
        .ok_or_else(|| "it does not run in a systemd service unit".to_owned())?;

    match is_plain_unit_name(unit) {
        true => Ok(ServerCgroup {
            unit: unit.to_owned(),
            path: path.to_owned(),
        }),
        false => Err(format!(
            "its cgroup names {unit:?}, which is not a unit name rastro will pass to systemctl"
        )),
    }
}

/// How `unit` starts the server, as systemd resolved it.
///
/// **Only once systemd puts the unit in the server's own cgroup.** The last component of a
/// cgroup path is a name anybody's user manager can also give a unit:
/// `user@1000.service/app.slice/redis-server.service` names the system unit too, whose file holds
/// the system server's password.
pub fn start_of(
    systemctl: &CanonicalTool,
    cgroup: &ServerCgroup,
) -> Result<ServerStart, CollectionError> {
    let unit = cgroup.unit.as_str();
    let group = systemctl.run(&[
        "show",
        "--property=ControlGroup",
        "--value",
        "--no-pager",
        END_OF_OPTIONS,
        unit,
    ])?;
    if group.trim_end() != cgroup.path {
        return Err(CollectionError::new(format!(
            "its cgroup is named like {unit}, but it does not run in {unit}, which systemd keeps \
             in another cgroup"
        )));
    }

    let mut arguments = SHOW.to_vec();
    arguments.extend([END_OF_OPTIONS, unit]);

    let shown = systemctl_show::parse(&systemctl.run(&arguments)?)?;
    let command = shown
        .into_values()
        .next()
        .and_then(|shown_unit| shown_unit.exec_start.into_iter().next())
        .ok_or_else(|| CollectionError::new(format!("systemd shows no ExecStart for {unit}")))?;

    // The unit enclosing a process is not always the one that started it: measured on GitHub's
    // runner, a redis a job starts runs in `hosted-compute-agent.service`.
    let program = Path::new(command.executable.as_str())
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    if ServerKind::from_program(program).is_none() {
        return Err(CollectionError::new(format!(
            "{unit} encloses the server, but its start command runs {}, which does not start a \
             redis server, so it is not the unit that started this one",
            command.executable.as_str()
        )));
    }

    Ok(start_from(command.argv.as_str()))
}

/// The configuration file and any command-line password in a start command.
///
/// **Split on whitespace, which is only safe because redis's own rule is refutable by it.**
/// systemd prints the vector without its quoting, so a file whose path holds a space would be cut
/// in two; redis takes its first argument as the file only when it does not start with `-`, and a
/// cut path fails to open rather than opening something else.
fn start_from(argv: &str) -> ServerStart {
    let words: Vec<&str> = argv.split_whitespace().collect();

    let config_file = words
        .get(1)
        .filter(|word| !word.starts_with('-'))
        .map(PathBuf::from);
    let password = words
        .windows(2)
        .filter(|pair| pair[0] == REQUIREPASS_OPTION)
        .map(|pair| pair[1].to_owned())
        .next_back();

    ServerStart {
        config_file,
        password,
    }
}

/// Whether a name is one systemd could have given a service, and nothing else.
fn is_plain_unit_name(unit: &str) -> bool {
    !unit.starts_with('-')
        && unit.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ':' | '_' | '.' | '@' | '-' | '\\')
        })
}
