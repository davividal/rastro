#![allow(dead_code)]

//! A box rebuilt from a redis matrix cell captured on a real server.
//!
//! The process files, socket tables and namespace links are the captured ones, byte for byte:
//! `fixtures/redis/cells/<cell>` was exported by `scripts/redis-matrix/export-fixtures.sh`, with
//! the one edit its README records. What a capture cannot carry is the server's filesystem, so
//! each server's `root` is rebuilt from the files the cell configured, read on the real box
//! inside that server's own root, at the same paths. `systemctl` answers from what the real one
//! said about each server's unit.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::canonical_tool::CanonicalTool;

use super::fs_tree::scratch_tree;
use super::shim;

/// The process files copied as they are; `fd` and the links are rebuilt from their text.
const PROCESS_FILES: [&str; 9] = [
    "cmdline",
    "comm",
    "stat",
    "status",
    "cgroup",
    "mountinfo",
    "net/tcp",
    "net/tcp6",
    "net/unix",
];

/// The links copied as the kernel spelled their targets, which need not exist here.
const PROCESS_LINKS: [&str; 5] = ["cwd", "exe", "ns/net", "ns/mnt", "ns/pid"];

/// A captured cell, rebuilt.
pub struct CapturedBox {
    pub proc: PathBuf,
    pub systemctl: CanonicalTool,
}

/// The directory a cell was exported to.
pub fn cell_directory(cell: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/redis/cells")
        .join(cell)
}

/// The captured servers of a cell, `server-1`, `server-2`, in capture order.
pub fn servers_of(cell: &str) -> Vec<PathBuf> {
    let mut servers: Vec<PathBuf> = fs::read_dir(cell_directory(cell))
        .expect("a captured cell")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("server-"))
                .is_some_and(|number| number.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .collect();
    servers.sort();
    servers
}

/// The process id the real box gave a captured server.
pub fn process_id_of(server: &Path) -> u32 {
    let stat = fs::read_to_string(server.join("process/stat")).expect("a captured stat");
    stat.split(' ')
        .next()
        .and_then(|id| id.parse().ok())
        .expect("a process id")
}

/// The cell's box: its `/proc` and a `systemctl` that knows its units.
pub fn captured_box(cell: &str) -> CapturedBox {
    // Named after the process and the test as well: two tests rebuilding one cell at once, in
    // one binary or under nextest in two, would each remove the other's tree.
    let test = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .replace("::", "-");
    let scratch = scratch_tree(
        &format!("redis-cell-{cell}-{}-{test}", std::process::id()),
        &["proc/net", "proc/self/ns", "bin"],
    );
    let captured = cell_directory(cell);
    let proc = scratch.join("proc");

    for table in ["tcp", "tcp6", "unix"] {
        fs::copy(
            captured.join("host-net").join(table),
            proc.join("net").join(table),
        )
        .expect("a captured socket table");
    }
    for (namespace, file) in [("mnt", "host-mntns.link"), ("net", "host-netns.link")] {
        link_as_captured(&captured.join(file), &proc.join("self/ns").join(namespace));
    }

    for server in servers_of(cell) {
        rebuild_process(&server, &proc, &scratch);
    }

    let systemctl = fake_systemctl(cell, &scratch.join("bin"));
    CapturedBox { proc, systemctl }
}

fn rebuild_process(server: &Path, proc: &Path, scratch: &Path) {
    let process_id = process_id_of(server);
    let process = proc.join(process_id.to_string());
    let captured = server.join("process");

    for file in PROCESS_FILES {
        let source = captured.join(file);
        if source.is_file() {
            let target = process.join(file);
            fs::create_dir_all(target.parent().expect("a parent")).expect("a scratch directory");
            fs::copy(&source, &target).expect("a copied process file");
        }
    }
    for link in PROCESS_LINKS {
        let source = captured.join(format!("{link}.link"));
        if source.is_file() {
            link_as_captured(&source, &process.join(link));
        }
    }
    descriptors_of(&captured.join("fd.list"), &process.join("fd"));

    // Arrange: the server's root, holding the files the cell configured at their own paths.
    let root = scratch.join(format!("root-{process_id}"));
    fs::create_dir_all(&root).expect("a scratch directory");
    let files = server.join("files");
    if files.is_dir() {
        copy_tree(&files, &root);
    }
    symlink(&root, process.join("root")).expect("a writable scratch symlink");
}

/// A link whose target is the text the real `readlink` gave.
fn link_as_captured(captured: &Path, link: &Path) {
    let target = fs::read_to_string(captured).expect("a captured link");
    fs::create_dir_all(link.parent().expect("a parent")).expect("a scratch directory");
    symlink(target.trim_end_matches('\n'), link).expect("a writable scratch symlink");
}

/// The process's open files as the kernel showed them, each a link to its `readlink` text.
fn descriptors_of(list: &Path, directory: &Path) {
    let Ok(text) = fs::read_to_string(list) else {
        return;
    };
    fs::create_dir_all(directory).expect("a scratch directory");
    for line in text.lines() {
        if let Some((number, target)) = line.split_once(" -> ")
            && !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
        {
            symlink(target, directory.join(number)).expect("a writable scratch symlink");
        }
    }
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("a captured directory").flatten() {
        let target = to.join(entry.file_name());
        match entry.path().is_dir() {
            true => {
                fs::create_dir_all(&target).expect("a scratch directory");
                copy_tree(&entry.path(), &target);
            }
            false => {
                fs::copy(entry.path(), &target).expect("a copied captured file");
            }
        }
    }
}

/// A `systemctl` that answers about each captured unit as the real one did: its `ControlGroup`
/// alone where asked for that value, and everything captured otherwise.
fn fake_systemctl(cell: &str, bin: &Path) -> CanonicalTool {
    for server in servers_of(cell) {
        let Ok(unit) = fs::read_to_string(server.join("unit")) else {
            continue;
        };
        let unit = unit.trim();
        let shown = fs::read_to_string(server.join("unit.show")).expect("a captured unit");
        fs::write(bin.join(format!("{unit}.show")), &shown).expect("a writable fixture");
        let group = shown
            .lines()
            .find_map(|line| line.strip_prefix("ControlGroup="))
            .unwrap_or_default();
        fs::write(bin.join(format!("{unit}.group")), format!("{group}\n"))
            .expect("a writable fixture");
    }

    shim::executable(
        bin,
        "systemctl",
        &format!(
            "#!/bin/sh\nfor unit; do :; done\ncase \"$*\" in\n  *--property=ControlGroup*) cat \"{bin}/$unit.group\" ;;\n  *) cat \"{bin}/$unit.show\" ;;\nesac\n",
            bin = bin.display()
        ),
    )
}
