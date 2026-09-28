#![allow(dead_code)]

//! A redis server that answers from a script, and a `/proc` that says it is running.
//!
//! The server records every command it receives, so a test can assert what rastro *sent*: a
//! failed `AUTH` is an entry in the real server's `ACL LOG`, which makes what goes over the wire
//! a question about changing the host rather than an implementation detail.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use redis_protocol::resp2::decode::decode;
use redis_protocol::resp2::types::OwnedFrame;

use super::fs_tree::{scratch_tree, write};

/// The inode the fixture's server socket is published under.
const SOCKET_INODE: u64 = 4242;

const TCP_HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const UNIX_HEADER: &str = "Num       RefCount Protocol Flags    Type St Inode Path\n";

/// A scripted server, listening on a unix socket.
pub struct FakeRedis {
    pub socket: PathBuf,
    received: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeRedis {
    /// A server answering each command, keyed by its words joined with spaces, with the bytes
    /// given. Anything unscripted is answered the way redis answers a command it lacks.
    pub fn answering(name: &str, script: &[(&str, &str)]) -> Self {
        Self::serving(name, script, None)
    }

    /// A stock server that wants `password` before it answers anything, as `requirepass` makes
    /// it: `NOAUTH` to every command until an `AUTH` with the password, `WRONGPASS` to one
    /// without it. Measured on Debian 12.
    pub fn stock_with_password(name: &str, password: &str, overrides: &[(&str, &str)]) -> Self {
        let info = bulk(DEBIAN_12_INFO);
        let config = debian_12_config();
        let replication = bulk(DEBIAN_12_REPLICATION);
        let acl = array_of(&["user default on nopass sanitize-payload ~* &* +@all"]);
        let mut script: BTreeMap<&str, &str> = [
            ("INFO server", info.as_str()),
            ("CONFIG GET *", config.as_str()),
            ("INFO replication", replication.as_str()),
            ("ACL LIST", acl.as_str()),
            ("MODULE LIST", "*0\r\n"),
        ]
        .into_iter()
        .collect();
        script.extend(overrides.iter().copied());

        Self::serving(
            name,
            &script.into_iter().collect::<Vec<_>>(),
            Some(password.to_owned()),
        )
    }

    fn serving(name: &str, script: &[(&str, &str)], password: Option<String>) -> Self {
        // A short path rather than the scratch tree: a unix socket path is capped at about a
        // hundred bytes, and the target directory of a worktree is most of that already.
        let socket = std::env::temp_dir().join(format!("rastro-{name}.sock"));
        let _ = fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("a bindable socket path");

        let script: BTreeMap<String, String> = script
            .iter()
            .map(|(command, reply)| ((*command).to_owned(), (*reply).to_owned()))
            .collect();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&received);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut authenticated = password.is_none();
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];

                while let Ok(read) = stream.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);

                    while let Ok(Some((frame, consumed))) = decode(&buffer) {
                        buffer.drain(..consumed);
                        let words = words_of(frame);
                        let is_auth = words[0].eq_ignore_ascii_case("AUTH");
                        let reply = match (&password, is_auth) {
                            (None, true) => "-ERR AUTH <password> called without any password \
                                             configured for the default user. Are you sure your \
                                             configuration is correct?\r\n"
                                .to_owned(),
                            (Some(expected), true) if words.last() == Some(expected) => {
                                authenticated = true;
                                "+OK\r\n".to_owned()
                            }
                            (Some(_), true) => "-WRONGPASS invalid username-password pair or \
                                                user is disabled.\r\n"
                                .to_owned(),
                            _ if !authenticated => {
                                "-NOAUTH Authentication required.\r\n".to_owned()
                            }
                            _ => script.get(&words.join(" ")).cloned().unwrap_or_else(|| {
                                format!("-ERR unknown command '{}'\r\n", words[0])
                            }),
                        };
                        recorder.lock().expect("an unpoisoned lock").push(words);
                        if stream.write_all(reply.as_bytes()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self { socket, received }
    }

    /// A server answering every read as a stock Debian 12 server would, each override replacing
    /// or adding one command's answer.
    ///
    /// So a test states only what is peculiar to its box, and a read added to the collector later
    /// does not turn every earlier test into one about a server that refuses it.
    pub fn stock(name: &str, overrides: &[(&str, &str)]) -> Self {
        let info = bulk(DEBIAN_12_INFO);
        let config = debian_12_config();
        let replication = bulk(DEBIAN_12_REPLICATION);
        let acl = array_of(&["user default on nopass sanitize-payload ~* &* +@all"]);
        let mut script: BTreeMap<&str, &str> = [
            ("INFO server", info.as_str()),
            ("CONFIG GET *", config.as_str()),
            ("INFO replication", replication.as_str()),
            ("ACL LIST", acl.as_str()),
            ("MODULE LIST", "*0\r\n"),
        ]
        .into_iter()
        .collect();
        script.extend(overrides.iter().copied());

        Self::answering(name, &script.into_iter().collect::<Vec<_>>())
    }

    /// Every command received so far, as its words.
    pub fn received(&self) -> Vec<Vec<String>> {
        self.received.lock().expect("an unpoisoned lock").clone()
    }

    /// A `/proc` in which a redis server holds this socket.
    pub fn proc(&self, name: &str) -> PathBuf {
        proc_holding(name, &self.socket)
    }
}

/// A `/proc` in which a redis server holds a unix socket at `socket`.
pub fn proc_holding(name: &str, socket: &Path) -> PathBuf {
    let proc = scratch_tree(name, &["proc/net", "proc/412/fd"]).join("proc");
    write(&proc, "net/tcp", TCP_HEADER);
    write(&proc, "net/tcp6", TCP_HEADER);
    write(
        &proc,
        "net/unix",
        &format!(
            "{UNIX_HEADER}0000000000000000: 00000002 00000000 00010000 0001 01 {SOCKET_INODE} {}\n",
            socket.display()
        ),
    );
    write(&proc, "412/comm", "redis-server\n");
    // Arrange: the Debian package's unit, as cgroup v2 names it.
    write(
        &proc,
        "412/cgroup",
        "0::/system.slice/redis-server.service\n",
    );
    write(&proc, "412/cmdline", "/usr/bin/redis-server unixsocket\0");
    symlink(format!("socket:[{SOCKET_INODE}]"), proc.join("412/fd/3"))
        .expect("a writable scratch symlink");

    proc
}

impl Drop for FakeRedis {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

fn words_of(frame: OwnedFrame) -> Vec<String> {
    match frame {
        OwnedFrame::Array(frames) => frames
            .into_iter()
            .map(|frame| match frame {
                OwnedFrame::BulkString(bytes) => String::from_utf8(bytes).expect("UTF-8"),
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

/// `INFO server` from a Debian 12 package, trimmed.
pub const DEBIAN_12_INFO: &str = "# Server\r\nredis_version:7.0.15\r\nredis_mode:standalone\r\nexecutable:/usr/bin/redis-server\r\nconfig_file:/etc/redis/redis.conf\r\n";

/// `INFO replication` from a stock server, which replicates nothing.
pub const DEBIAN_12_REPLICATION: &str = "# Replication\r\nrole:master\r\nconnected_slaves:0\r\n";

/// An array reply of bulk strings, which is how `CONFIG GET` answers.
pub fn array_of(items: &[&str]) -> String {
    let elements: String = items.iter().map(|item| bulk(item)).collect();

    format!("*{}\r\n{elements}", items.len())
}

/// `CONFIG GET *` from a stock server, trimmed to three settings.
pub fn debian_12_config() -> String {
    array_of(&[
        "bind",
        "127.0.0.1 -::1",
        "port",
        "6379",
        "save",
        "3600 1 300 100 60 10000",
    ])
}

/// What a server says about a command renamed away. Measured on Debian 12 and redis 8.
pub fn unknown_command(command: &str) -> String {
    format!("-ERR unknown command '{command}', with args beginning with: \r\n")
}

/// The socket path a fixture server's `/proc` names, as the facet keys it.
pub fn key_of(server: &FakeRedis) -> String {
    server.socket.display().to_string()
}

/// A path nothing listens on, for a server the kernel says holds a socket that refuses.
pub fn dead_socket(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rastro-{name}.sock"));
    let _ = fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("a bindable socket path");
    drop(listener);

    path
}
