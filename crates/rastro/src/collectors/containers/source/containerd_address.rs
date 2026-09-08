//! Where a containerd on this box is listening.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use rastro_collector::AbsolutePath;

/// The socket containerd documents as its default, and the only value here that is not read
/// from the box.
const DEFAULT_ADDRESS: &str = "/run/containerd/containerd.sock";

/// What the binary is called, which is how its process is picked out of `/proc`.
const PROGRAM: &str = "containerd";

/// The flag a containerd may be given to move its socket.
const ADDRESS_FLAG: &str = "--address";

/// The flag naming the file the socket is otherwise in.
const CONFIG_FLAG: &str = "--config";

/// Finding the address of the containerd this box is running.
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
pub struct ContainerdAddress;

impl ContainerdAddress {
    /// The address on this host, or nothing if no containerd is running.
    pub fn discover() -> Option<AbsolutePath> {
        Self::under("/proc")
    }

    /// The same over a `/proc` the caller chose, which is what the tests hand it.
    pub fn under(proc: impl AsRef<Path>) -> Option<AbsolutePath> {
        let arguments = command_line_of(proc.as_ref())?;

        let address = flag_value(&arguments, ADDRESS_FLAG)
            .or_else(|| {
                flag_value(&arguments, CONFIG_FLAG).and_then(|config| serving_address(&config))
            })
            .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());

        AbsolutePath::new(address, "containerd address").ok()
    }
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

/// The two lines of a containerd configuration this read is about.
///
/// **Named sections rather than a search through the document**, which is the whole point:
/// the first `address =` in the file docker's containerd is given belongs to `[debug]`, and
/// the debug endpoint answers a different API. Everything else in the file is ignored by
/// serde, which is what makes this a two-line reader of a large configuration rather than a
/// parser of one.
#[derive(Debug, Deserialize)]
struct ContainerdConfiguration {
    grpc: Option<ServingSection>,
}

#[derive(Debug, Deserialize)]
struct ServingSection {
    address: Option<String>,
}

/// The serving address inside a containerd configuration.
///
/// A file rastro cannot read or cannot parse yields nothing, and the caller falls back to
/// the documented default: the engine is plainly running, so the default is a better answer
/// than none, and `ctr` says so loudly if it is the wrong one.
fn serving_address(config: &str) -> Option<String> {
    let text = fs::read_to_string(config).ok()?;
    let parsed: ContainerdConfiguration = toml::from_str(&text).ok()?;

    parsed.grpc?.address
}
