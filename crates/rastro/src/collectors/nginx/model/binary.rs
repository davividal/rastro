//! The nginx that would serve, and what it was built to read.

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

    fn argument(&self, name: &str) -> Option<&str> {
        self.configure_arguments
            .iter()
            .find_map(|argument| argument.as_str().strip_prefix(name))
    }
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
