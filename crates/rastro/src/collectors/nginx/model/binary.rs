//! The nginx that would serve, and what it was built to read.

use std::path::Path;

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::nginx::value_objects::{BuildVersion, ConfigureArgument};

/// Where nginx looks for its configuration when nobody says otherwise.
///
/// nginx's own compiled-in defaults, used only when the binary reports no `--prefix` or
/// `--conf-path` — which every distribution's build does report. They are here rather than
/// in the source layer because they are facts about nginx, not about the banner it prints.
const DEFAULT_PREFIX: &str = "/usr/local/nginx";
const DEFAULT_CONFIGURATION: &str = "conf/nginx.conf";

const PREFIX_ARGUMENT: &str = "--prefix=";

/// One tree nginx writes into: the configure argument that names it, and what nginx uses
/// when the build never named it.
///
/// The fallbacks are nginx's own compiled defaults, relative to the prefix. They are here
/// because a build from source with no switches still writes into all five, and a facet that
/// only knew the ones a distribution spells out would leave them unclaimed on exactly the
/// hosts nobody packaged.
struct TemporaryTree {
    argument: &'static str,
    fallback: &'static str,
}

const TEMPORARY_TREES: [TemporaryTree; 5] = [
    TemporaryTree {
        argument: "--http-client-body-temp-path=",
        fallback: "client_body_temp",
    },
    TemporaryTree {
        argument: "--http-proxy-temp-path=",
        fallback: "proxy_temp",
    },
    TemporaryTree {
        argument: "--http-fastcgi-temp-path=",
        fallback: "fastcgi_temp",
    },
    TemporaryTree {
        argument: "--http-uwsgi-temp-path=",
        fallback: "uwsgi_temp",
    },
    TemporaryTree {
        argument: "--http-scgi-temp-path=",
        fallback: "scgi_temp",
    },
];
const CONFIGURATION_ARGUMENT: &str = "--conf-path=";

/// The binary, as it describes itself.
///
/// Asked with `-V`, which prints the banner and exits without opening a configuration. That
/// matters: it is the one thing rastro can ask nginx that costs the host nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binary {
    pub path: AbsolutePath,
    pub product: NonEmptyText,
    pub version: BuildVersion,
    pub compiler: Option<NonEmptyText>,
    pub tls_library: Option<NonEmptyText>,
    pub configure_arguments: Vec<ConfigureArgument>,
}

impl Binary {
    /// What relative paths in the configuration are relative to.
    pub fn prefix(&self) -> String {
        self.argument(PREFIX_ARGUMENT)
            .unwrap_or(DEFAULT_PREFIX)
            .to_owned()
    }

    /// The configuration this binary would read, absent a `-c` on the command line.
    pub fn configuration_path(&self) -> String {
        self.argument(CONFIGURATION_ARGUMENT)
            .unwrap_or(DEFAULT_CONFIGURATION)
            .to_owned()
    }

    /// The working trees this binary was *built* to use, which are what it uses unless a
    /// directive says otherwise.
    ///
    /// All five, always: a build that named none of them still writes into all of them, at
    /// nginx's own defaults under the prefix. Each is resolved against that prefix, because
    /// a configure argument may be relative and a relative tree is one the walk cannot be
    /// told to step back from.
    pub fn working_trees(&self) -> Vec<String> {
        let prefix = self.prefix();
        let mut found: Vec<String> = TEMPORARY_TREES
            .iter()
            .map(|tree| self.argument(tree.argument).unwrap_or(tree.fallback))
            .map(|path| under(&prefix, path))
            .collect();

        found.sort();
        found.dedup();
        found
    }

    fn argument(&self, name: &str) -> Option<&str> {
        self.configure_arguments
            .iter()
            .find_map(|argument| argument.as_str().strip_prefix(name))
    }
}

/// A path as nginx would use it: relative ones hang off the prefix.
fn under(prefix: &str, path: &str) -> String {
    Path::new(prefix).join(path).to_string_lossy().into_owned()
}

impl From<&Binary> for Observation {
    fn from(binary: &Binary) -> Self {
        Observation::object([
            (
                "compiler",
                binary
                    .compiler
                    .as_ref()
                    .map_or_else(Observation::null, |compiler| {
                        Observation::text(compiler.as_str())
                    }),
            ),
            (
                "configure_arguments",
                Observation::list(binary.configure_arguments.iter().map(Observation::from)),
            ),
            ("path", Observation::text(binary.path.as_str())),
            ("product", Observation::text(binary.product.as_str())),
            (
                "tls_library",
                binary
                    .tls_library
                    .as_ref()
                    .map_or_else(Observation::null, |library| {
                        Observation::text(library.as_str())
                    }),
            ),
            ("version", Observation::from(&binary.version)),
        ])
    }
}
