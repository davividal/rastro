//! What a node says about itself when asked.

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::Listener;

/// A node's own account of what it is running and which files it is running from.
///
/// **The node is asked rather than its configuration read**, which is the Layer 3 preference
/// and here it is not even a preference: a stock Debian install has no `rabbitmq.conf` at
/// all, so there is nothing to read and the node's answer is the only account of its
/// effective state that exists.
///
/// **The volatile half of `status` is not read.** Memory, uptime, the operating-system pid,
/// the run queue, descriptor counts, free disk and connection totals all move between two
/// runs of a box nobody touched. They are not annotated volatile and dropped at render time,
/// they never enter the model: a fingerprint is not a monitoring tool, and the cheapest way
/// to be sure a value cannot leak into a diff is not to read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeStatus {
    /// The node's own name, as its listeners report it.
    ///
    /// The status document carries no name of its own. This is kept beside the name rastro
    /// composed from the register and the box's hostname precisely so a disagreement between
    /// the two is visible: a node under long names calls itself something rastro would not
    /// have guessed.
    pub reported_name: Option<String>,

    pub rabbitmq_version: String,

    /// The whole Erlang banner, as the node prints it.
    ///
    /// Kept entire rather than reduced to `27`, because the rest of it is state too: the erts
    /// version, whether the VM has the JIT, and how many schedulers it started.
    pub erlang_version: String,

    /// The TLS library the node loaded, where it says.
    pub crypto_library_version: Option<String>,

    /// The commercial build's own name and version, absent on the open-source one.
    ///
    /// Both print as `""` there, and an empty string asserts something the node did not say,
    /// so they are mapped to absent at the boundary.
    pub product_name: Option<String>,
    pub product_version: Option<String>,

    pub operating_system: String,

    pub data_directory: String,

    /// Where the quorum queues keep their Raft logs, which is usually under the data
    /// directory and does not have to be.
    pub raft_data_directory: Option<String>,

    /// The configuration files the node actually read, in the order it read them.
    ///
    /// Empty on a stock install, which is a reading rather than a gap: rastro asked and the
    /// node named none.
    pub configuration_files: Vec<String>,

    /// Where the node's log goes.
    ///
    /// Destinations rather than files, because `<stdout>` appears in the list beside a real
    /// path and is not a file at all.
    pub log_destinations: Vec<String>,

    /// The file that says which plugins are enabled, whether or not it exists.
    pub enabled_plugins_file: Option<String>,

    /// The plugins the node is actually running, which is the effective half of that file.
    pub active_plugins: Vec<String>,

    pub listeners: Vec<Listener>,

    /// The node's own tags, which are configuration and not observation.
    pub tags: Vec<String>,

    pub under_maintenance: bool,

    /// How long the node waits before calling a silent peer down, in seconds.
    pub net_tick_seconds: Option<i64>,

    /// The memory ceiling in bytes, as the node resolved it.
    ///
    /// The *setting* beside it is `{"relative": 0.6}` and the document admits no floating
    /// point, so what is recorded is the resolved limit. It is state under either spelling:
    /// it moves when the setting moves, and when the box gains memory.
    pub memory_high_watermark_limit: Option<i64>,

    /// The free-disk floor in bytes, below which the node blocks publishers.
    pub disk_free_limit: Option<i64>,
}

impl From<&NodeStatus> for Observation {
    fn from(status: &NodeStatus) -> Self {
        Observation::object([
            ("reported_name", optional_text(&status.reported_name)),
            (
                "rabbitmq_version",
                Observation::text(status.rabbitmq_version.as_str()),
            ),
            (
                "erlang_version",
                Observation::text(status.erlang_version.as_str()),
            ),
            (
                "crypto_library_version",
                optional_text(&status.crypto_library_version),
            ),
            ("product_name", optional_text(&status.product_name)),
            ("product_version", optional_text(&status.product_version)),
            (
                "operating_system",
                Observation::text(status.operating_system.as_str()),
            ),
            (
                "data_directory",
                Observation::text(status.data_directory.as_str()),
            ),
            (
                "raft_data_directory",
                optional_text(&status.raft_data_directory),
            ),
            ("configuration_files", texts(&status.configuration_files)),
            ("log_destinations", texts(&status.log_destinations)),
            (
                "enabled_plugins_file",
                optional_text(&status.enabled_plugins_file),
            ),
            ("active_plugins", texts(&status.active_plugins)),
            (
                "listeners",
                Observation::list(status.listeners.iter().map(Observation::from)),
            ),
            ("tags", texts(&status.tags)),
            (
                "under_maintenance",
                Observation::boolean(status.under_maintenance),
            ),
            (
                "net_tick_seconds",
                optional_integer(status.net_tick_seconds),
            ),
            (
                "memory_high_watermark_limit",
                optional_integer(status.memory_high_watermark_limit),
            ),
            ("disk_free_limit", optional_integer(status.disk_free_limit)),
        ])
    }
}

fn optional_text(value: &Option<String>) -> Observation {
    match value {
        Some(text) => Observation::text(text.as_str()),
        None => Observation::null(),
    }
}

fn optional_integer(value: Option<i64>) -> Observation {
    match value {
        Some(number) => Observation::integer(number),
        None => Observation::null(),
    }
}

/// A list of text, which is how every one of this type's collections renders.
///
/// Order is the node's own everywhere it is used: the configuration files are in the order
/// they were read, and the log destinations in the order they are written to. Sorting either
/// would destroy the one thing they say beyond their contents.
fn texts(values: &[String]) -> Observation {
    Observation::list(values.iter().map(|value| Observation::text(value.as_str())))
}
