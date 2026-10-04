#![allow(dead_code)]

//! A `/proc` rebuilt from a matrix cell captured on a real node.
//!
//! The process files are the captured ones, byte for byte: `fixtures/elasticsearch/cells/<cell>`
//! was exported by `scripts/elasticsearch-matrix/export-fixtures.sh`, except for the one edit
//! its README records. What a capture cannot carry is the node's filesystem, so each node's
//! `root` is rebuilt from the cell's setup: its install with the server jar under `lib/`, and
//! its config directory holding the `elasticsearch.yml` read from the real node.

use std::fs;
use std::path::{Path, PathBuf};

use super::fs_tree::{scratch_tree, write};

/// Where a cell's setup put one node, and which release it runs.
pub struct Install {
    /// The captured node's directory, `node-1`, `node-2`, in capture order.
    pub node: &'static str,
    pub home: &'static str,
    pub config: &'static str,
    pub version: &'static str,
}

/// The `/proc` a cell's capture describes, with each node's server and parent process.
pub fn captured_proc(cell: &str, installs: &[Install]) -> PathBuf {
    // Named after the process and the test as well: two tests rebuilding one cell's tree at
    // once, in one binary or under nextest in two, would each remove the other's.
    let test = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .replace("::", "-");
    let process = std::process::id();
    let proc = scratch_tree(&format!("elasticsearch-cell-{cell}-{process}-{test}"), &[]);
    let captured = cell_directory(cell);

    for install in installs {
        let node = captured.join(install.node);
        let process_id = process_id_in(&node.join("server/stat"));
        copy_process(&node.join("server"), &proc.join(&process_id));

        if node.join("parent/stat").is_file() {
            let parent_id = process_id_in(&node.join("parent/stat"));
            copy_process(&node.join("parent"), &proc.join(parent_id));
        }

        let root = proc.join(&process_id).join("root");
        let jar = format!(
            "{}/lib/elasticsearch-{}.jar",
            relative(install.home),
            install.version
        );
        write(&root, &jar, "");
        let file = node.join("files/elasticsearch.yml");
        if let Ok(text) = fs::read_to_string(&file) {
            write(
                &root,
                &format!("{}/elasticsearch.yml", relative(install.config)),
                &text,
            );
        }
    }

    proc
}

/// The captured nodes of a cell, so a test can check its table names every one of them.
pub fn captured_nodes(cell: &str) -> Vec<String> {
    let mut nodes: Vec<String> = fs::read_dir(cell_directory(cell))
        .expect("a captured cell")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("node-"))
        .collect();
    nodes.sort();
    nodes
}

fn cell_directory(cell: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/elasticsearch/cells")
        .join(cell)
}

fn process_id_in(stat: &Path) -> String {
    let text = fs::read_to_string(stat).expect("a captured stat");
    text.split(' ').next().expect("a process id").to_owned()
}

fn copy_process(captured: &Path, process: &Path) {
    fs::create_dir_all(process).expect("a writable scratch directory");
    for file in ["cmdline", "environ", "stat", "status"] {
        let source = captured.join(file);
        if source.is_file() {
            fs::copy(&source, process.join(file)).expect("a copied process file");
        }
    }
}

fn relative(absolute: &str) -> &str {
    absolute.trim_start_matches('/')
}

const IMAGE: &str = "/usr/share/elasticsearch";
const IMAGE_CONFIG: &str = "/usr/share/elasticsearch/config";
const PACKAGE_CONFIG: &str = "/etc/elasticsearch";

/// One node of a cell's table, as a literal so the table can be borrowed for `'static`.
macro_rules! install {
    ($node:literal, $home:expr, $config:expr, $version:literal) => {
        Install {
            node: $node,
            home: $home,
            config: $config,
            version: $version,
        }
    };
}

/// Each cell's nodes as `docs/elasticsearch-matrix.md` and `scripts/elasticsearch-matrix/cells.sh` set them up.
///
/// Facts of the setup, not readings of the capture: the package installs under
/// `/usr/share/elasticsearch` and configures `/etc/elasticsearch`, the image keeps both under
/// `/usr/share/elasticsearch`, and each tarball cell extracted into its own `/opt/es-<cell>`.
pub fn installs_of(cell: &str) -> &'static [Install] {
    match cell {
        "01" => &[install!("node-1", IMAGE, PACKAGE_CONFIG, "7.17.29")],
        "02" | "03" => &[install!("node-1", IMAGE, PACKAGE_CONFIG, "8.19.22")],
        "04" => &[install!("node-1", IMAGE, "/srv/es/config", "9.5.4")],
        "05" => &[install!(
            "node-1",
            "/opt/es-05",
            "/opt/es-05/config",
            "9.4.7"
        )],
        "06" => &[install!(
            "node-1",
            "/opt/es-06",
            "/opt/es-06/config",
            "8.19.22"
        )],
        "07" => &[install!(
            "node-1",
            "/opt/es-07",
            "/opt/es-07/config",
            "7.17.29"
        )],
        "08" => &[install!(
            "node-1",
            "/opt/es-08",
            "/opt/es-08/config",
            "9.5.4"
        )],
        "09" | "24" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "9.5.4")],
        "10" | "12" | "15" | "26" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "8.19.22")],
        "11" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "9.4.7")],
        "13" | "22" => &[
            install!("node-1", IMAGE, IMAGE_CONFIG, "9.5.4"),
            install!("node-2", IMAGE, IMAGE_CONFIG, "9.5.4"),
        ],
        "14" => &[
            install!("node-1", "/opt/es-14a", "/opt/es-14a/config", "7.17.29"),
            install!("node-2", "/opt/es-14b", "/opt/es-14b/config", "7.17.29"),
        ],
        "16" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "6.8.23")],
        "17" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "7.10.2")],
        "18" => &[install!("node-1", IMAGE, PACKAGE_CONFIG, "8.15.3")],
        "19" => &[install!("node-1", IMAGE, IMAGE_CONFIG, "9.2.0")],
        "20" => &[],
        "21" => &[
            install!("node-1", IMAGE, PACKAGE_CONFIG, "8.19.22"),
            install!("node-2", IMAGE, IMAGE_CONFIG, "9.5.4"),
        ],
        "23" => &[
            install!("node-1", "/opt/es-23a", "/opt/es-23a/config", "7.17.29"),
            install!("node-2", "/opt/es-23b", "/opt/es-23b/config", "7.17.29"),
        ],
        "25" => &[install!("node-1", IMAGE, PACKAGE_CONFIG, "9.4.7")],
        other => panic!("cell {other} is not in the matrix"),
    }
}
