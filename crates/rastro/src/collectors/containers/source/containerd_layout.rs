//! Where a containerd on this box is listening.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use rastro_collector::AbsolutePath;

/// The three places containerd documents as its defaults, and the only values here that are
/// not read from the box.
const DEFAULT_ADDRESS: &str = "/run/containerd/containerd.sock";
const DEFAULT_ROOT: &str = "/var/lib/containerd";
const DEFAULT_STATE: &str = "/run/containerd";

/// What the binary is called, which is how its process is picked out of `/proc`.
const PROGRAM: &str = "containerd";

/// The flag a containerd may be given to move its socket.
const ADDRESS_FLAG: &str = "--address";

/// The flag naming the file the socket is otherwise in.
const CONFIG_FLAG: &str = "--config";

/// Where the containerd this box is running lives.
///
/// **`ctr`'s own default is wrong on any box that has docker**, which is what makes this
/// worth a type. Measured on docker 29.8.0: containerd is started as
/// `containerd --config /var/run/docker/containerd/containerd.toml`, its socket is
/// `/var/run/docker/containerd/containerd.sock`, and `/run/containerd/containerd.sock` — where
/// `ctr` looks when nobody tells it otherwise — does not exist at all. A bare `ctr` on that
/// box fails outright.
///
/// So the running process is asked instead, in the order it can answer:
///
/// 1. its own `--address`, which is the whole answer when it is there;
/// 2. the `[grpc] address` of the file its `--config` names;
/// 3. containerd's documented default, for a containerd started with neither.
///
/// **The configuration is parsed rather than searched, and that is not fastidiousness.** The
/// first `address =` in the file docker's containerd is given belongs to `[debug]`, and the
/// debug endpoint answers a different API. Anything taking the first match would talk to the
/// wrong socket and report the failure as though containerd were broken.
///
/// This is a *discovery* read, not a state read, which is why parsing a configuration here
/// does not need the licence the nginx entry in `docs/decisions.md` grants: it establishes
/// how to reach the service, and what the service then says about itself is asked of the
/// service.
/// Everything about a containerd that has to be known before it is asked anything: the
/// socket to ask at, and the two directories it keeps what it holds in.
///
/// Every field is absent on a box with no containerd running, and all three come from one
/// read of the process and its configuration rather than three.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContainerdLayout {
    pub address: Option<AbsolutePath>,
    /// Where the content store and the snapshots are.
    pub root: Option<AbsolutePath>,
    /// Where the shims, sockets and task directories are, which is runtime state.
    pub state: Option<AbsolutePath>,
}

impl ContainerdLayout {
    /// The layout on this host, empty if no containerd is running.
    pub fn discover() -> Self {
        Self::under("/proc")
    }

    /// The same over a `/proc` the caller chose, which is what the tests hand it.
    pub fn under(proc: impl AsRef<Path>) -> Self {
        let Some(arguments) = command_line_of(proc.as_ref()) else {
            return Self::default();
        };

        let configured = flag_value(&arguments, CONFIG_FLAG).map(Configuration::read);
        let configured = configured.unwrap_or_default();

        let address = flag_value(&arguments, ADDRESS_FLAG)
            .or(configured.address)
            .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());

        Self {
            // **Not resolved, unlike the two directories below.** The address is handed to
            // `ctr`, which follows a symlink itself, and what rastro records is where it
            // asked. The directories are handed to nothing: they are compared against paths
            // the filesystem walk produces, and it never follows a symlink.
            address: AbsolutePath::new(address, "containerd address").ok(),
            root: as_walked(
                configured.root.unwrap_or_else(|| DEFAULT_ROOT.to_owned()),
                "containerd root",
            ),
            state: as_walked(
                configured.state.unwrap_or_else(|| DEFAULT_STATE.to_owned()),
                "containerd state",
            ),
        }
    }
}

/// A directory as the filesystem walk would see it.
///
/// **Resolved through its symlinks, because otherwise a claim over it is a rule about a tree
/// nothing visits.** docker gives its containerd `state = "/var/run/docker/containerd/daemon"`,
/// and on Debian `/var/run` is a symlink to `/run`; the walk never follows a symlink, so it
/// only ever records the real path. A path that cannot be resolved is kept as reported, since
/// a declared rule that matches nothing is still better than a silent omission.
///
/// This is why the socket address is *not* put through here: it is handed to `ctr` rather
/// than compared with the walk, and resolving it would change what rastro records about
/// where it asked.
fn as_walked(path: String, kind: &str) -> Option<AbsolutePath> {
    let resolved = fs::canonicalize(&path)
        .ok()
        .and_then(|resolved| resolved.to_str().map(str::to_owned))
        .unwrap_or(path);

    AbsolutePath::new(resolved, kind).ok()
}

/// The command line of the containerd running here, if one is.
///
/// The executable behind the process is what identifies it, for the reason the exporters
/// facet gives about units: a name is whatever somebody chose, and the binary is the fact.
fn command_line_of(proc: &Path) -> Option<Vec<String>> {
    for entry in fs::read_dir(proc).ok()?.flatten() {
        let path = entry.path();
        if !path
            .file_name()?
            .to_str()?
            .chars()
            .all(|c| c.is_ascii_digit())
        {
            continue;
        }

        let Ok(executable) = fs::read_link(path.join("exe")) else {
            continue;
        };
        if executable.file_name().and_then(|name| name.to_str()) != Some(PROGRAM) {
            continue;
        }

        let Ok(raw) = fs::read(path.join("cmdline")) else {
            continue;
        };

        return Some(
            String::from_utf8_lossy(&raw)
                .split('\0')
                .filter(|argument| !argument.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }

    None
}

/// The value of a flag written either way round: `--flag value` or `--flag=value`.
fn flag_value(arguments: &[String], flag: &str) -> Option<String> {
    let mut arguments = arguments.iter();

    while let Some(argument) = arguments.next() {
        if argument == flag {
            return arguments.next().cloned();
        }

        if let Some(value) = argument.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_owned());
        }
    }

    None
}

/// The four lines of a containerd configuration this read is about.
///
/// **Named sections rather than a search through the document**, which is the whole point:
/// the first `address =` in the file docker's containerd is given belongs to `[debug]`, and
/// the debug endpoint answers a different API. Everything else in the file is ignored by
/// serde, which is what makes this a two-line reader of a large configuration rather than a
/// parser of one.
#[derive(Debug, Default, Deserialize)]
struct ContainerdConfiguration {
    root: Option<String>,
    state: Option<String>,
    grpc: Option<ServingSection>,
}

#[derive(Debug, Deserialize)]
struct ServingSection {
    address: Option<String>,
}

/// What one configuration file said, with everything it did not say left absent.
#[derive(Debug, Default)]
struct Configuration {
    address: Option<String>,
    root: Option<String>,
    state: Option<String>,
}

impl Configuration {
    /// Reads the file, or nothing from it.
    ///
    /// A file rastro cannot read or cannot parse yields nothing at all, and the caller falls
    /// back to the documented defaults: the engine is plainly running, so the defaults are a
    /// better answer than none, and `ctr` says so loudly if the address is wrong.
    fn read(path: String) -> Self {
        let Some(parsed) = fs::read_to_string(&path)
            .ok()
            .and_then(|text| toml::from_str::<ContainerdConfiguration>(&text).ok())
        else {
            return Self::default();
        };

        Self {
            address: parsed.grpc.and_then(|grpc| grpc.address),
            root: parsed.root,
            state: parsed.state,
        }
    }
}
