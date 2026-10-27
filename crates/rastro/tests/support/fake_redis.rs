#![allow(dead_code)]

//! A redis server that answers from a script, and a `/proc` that says it is running.
//!
//! The server records every command it receives, so a test can assert what rastro *sent*: a
//! failed `AUTH` is an entry in the real server's `ACL LOG`, which makes what goes over the wire
//! a question about changing the host rather than an implementation detail.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use redis::{Parser, Value};

use super::captured_redis_cell::captured_reply;
use super::fs_tree::{scratch_tree, write};

/// The inode the fixture's server socket is published under.
const SOCKET_INODE: u64 = 4242;

const TCP_HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// A scripted server, listening on a loopback port.
pub struct FakeRedis {
    pub port: u16,
    received: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeRedis {
    /// A server answering each command, keyed by its words joined with spaces, with the bytes
    /// given. Anything unscripted is answered the way redis answers a command it lacks.
    pub fn answering(script: &[(&str, &str)]) -> Self {
        Self::serving(owned(script.iter().copied()))
    }

    fn serving(script: BTreeMap<String, String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();

        let received = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&received);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut parser = Parser::new();
                let Ok(mut reading) = stream.try_clone() else {
                    return;
                };

                // One command at a time until the client hangs up, which ends the parse.
                while let Ok(command) = parser.parse_value(&mut reading) {
                    let words = words_of(command);
                    let reply = script
                        .get(&words.join(" "))
                        .cloned()
                        .unwrap_or_else(|| format!("-ERR unknown command '{}'\r\n", words[0]));
                    recorder.lock().expect("an unpoisoned lock").push(words);
                    if stream.write_all(reply.as_bytes()).is_err() {
                        break;
                    }
                }
            }
        });

        Self { port, received }
    }

    /// A server answering every read as a stock Debian 12 server would, each override replacing
    /// or adding one command's answer.
    ///
    /// So a test states only what is peculiar to its box, and a read added to the collector later
    /// does not turn every earlier test into one about a server that refuses it.
    pub fn stock(overrides: &[(&str, &str)]) -> Self {
        Self::serving(stock_script(overrides))
    }

    /// Every command received so far, as its words.
    pub fn received(&self) -> Vec<Vec<String>> {
        self.received.lock().expect("an unpoisoned lock").clone()
    }

    /// A `/proc` in which a redis server holds this socket.
    pub fn proc(&self, name: &str) -> PathBuf {
        proc_holding(name, self.port)
    }
}

/// The pid a fixture `/proc` gives the server: this test process, which is what the kernel names
/// as the holder of a socket the fake server listens on.
pub fn server_pid() -> String {
    std::process::id().to_string()
}

/// A `/proc` in which a redis server, this test process, listens on `127.0.0.1:<port>`.
pub fn proc_holding(name: &str, port: u16) -> PathBuf {
    let pid = server_pid();
    let pid = pid.as_str();
    let proc = scratch_tree(name, &["proc/net", &format!("proc/{pid}/fd")]).join("proc");
    write(
        &proc,
        "net/tcp",
        &format!(
            "{TCP_HEADER}   0: 0100007F:{port:04X} 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 {SOCKET_INODE} 1 0000000000000000 100 0 0 10 0\n"
        ),
    );
    write(&proc, "net/tcp6", TCP_HEADER);
    write(&proc, &format!("{pid}/comm"), "redis-server\n");
    // Arrange: the Debian package's unit, as cgroup v2 names it.
    write(
        &proc,
        &format!("{pid}/cgroup"),
        "0::/system.slice/redis-server.service\n",
    );
    write(
        &proc,
        &format!("{pid}/cmdline"),
        &format!("/usr/bin/redis-server 127.0.0.1:{port}\0"),
    );
    symlink(
        format!("socket:[{SOCKET_INODE}]"),
        proc.join(pid).join("fd/3"),
    )
    .expect("a writable scratch symlink");
    // Arrange: the server in rastro's own root, so its paths mean the same on both sides.
    symlink("/", proc.join(pid).join("root")).expect("a writable scratch symlink");
    // Arrange: the server and rastro in one mount and one network namespace, as on the host.
    for process in [pid, "self"] {
        fs::create_dir_all(proc.join(process).join("ns")).expect("a writable scratch directory");
        symlink(HOST_MOUNT_NAMESPACE, proc.join(process).join("ns/mnt"))
            .expect("a writable scratch symlink");
        symlink(HOST_NETWORK_NAMESPACE, proc.join(process).join("ns/net"))
            .expect("a writable scratch symlink");
    }

    proc
}

/// The mount namespace a fixture's server and rastro share, as `/proc/<pid>/ns/mnt` names it.
pub const HOST_MOUNT_NAMESPACE: &str = "mnt:[4026531841]";

/// The network namespace a fixture's server and rastro share, as `/proc/<pid>/ns/net` names it.
pub const HOST_NETWORK_NAMESPACE: &str = "net:[4026531840]";

/// What a stock server answers to every read rastro makes, each override replacing or adding one
/// command's answer: redis 8.10 from the redis.io package, captured as cell 01 of the matrix.
fn stock_script(overrides: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut script: BTreeMap<String, String> = STOCK_COMMANDS
        .iter()
        .map(|command| ((*command).to_owned(), captured_reply(STOCK_CELL, command)))
        .collect();
    script.extend(owned(overrides.iter().copied()));

    script
}

/// The cell the stock server replays: the packaged default, the matrix's baseline.
const STOCK_CELL: &str = "01";

/// Every command rastro sends a server that answers, in the order it sends them.
const STOCK_COMMANDS: [&str; 5] = [
    "INFO server",
    "CONFIG GET *",
    "INFO replication",
    "ACL LIST",
    "MODULE LIST",
];

fn owned<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(command, reply)| (command.to_owned(), reply.to_owned()))
        .collect()
}

fn words_of(command: Value) -> Vec<String> {
    match command {
        Value::Array(frames) => frames
            .into_iter()
            .map(|frame| match frame {
                Value::BulkString(bytes) => String::from_utf8(bytes).expect("UTF-8"),
                other => panic!("a command word that is not a bulk string: {other:?}"),
            })
            .collect(),
        other => panic!("a command that is not an array: {other:?}"),
    }
}

/// A bulk string reply, which is how `INFO` and every text value arrive.
pub fn bulk(text: &str) -> String {
    format!("${}\r\n{text}\r\n", text.len())
}

/// An array reply of bulk strings, which is how `CONFIG GET` answers.
pub fn array_of(items: &[&str]) -> String {
    let elements: String = items.iter().map(|item| bulk(item)).collect();

    format!("*{}\r\n{elements}", items.len())
}

/// What a server says about a command renamed away. Measured on Debian 12 and redis 8.
pub fn unknown_command(command: &str) -> String {
    format!("-ERR unknown command '{command}', with args beginning with: \r\n")
}

/// The port a fixture server listens on, as the facet keys it.
pub fn key_of(server: &FakeRedis) -> String {
    server.port.to_string()
}

/// A loopback port nothing listens on, for a server the kernel says holds a socket that refuses.
pub fn dead_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port")
        .port()
}
