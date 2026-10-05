#![allow(dead_code)]

//! A stand-in Elasticsearch node: a real listener that answers fixed routes, and a `/proc` that
//! points at it the way the kernel would point at a real node.
//!
//! A real socket, because what the collector does is send bytes, and every request it sends is
//! recorded, so a test can say that nothing was sent where nothing may be.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use super::fs_tree::{scratch_tree, write};

/// The pid every fixture's server has.
pub const PID: &str = "600";

const SOCKET_INODE: u64 = 4242;

/// The first byte of a TLS handshake record, which a ClientHello opens with.
const TLS_HANDSHAKE: u8 = 0x16;

/// A 7.x-shaped server from the docker image, which carries its own paths and whose environment
/// holds settings; the 8.x launcher split is the residency read's to test.
const SERVER_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0\
    -Des.path.home=/usr/share/elasticsearch\0-Des.path.conf=/etc/elasticsearch\0\
    -Des.distribution.type=docker\0\
    -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0";

const TCP_HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// The release every fixture's server has installed, which [`ROOT`] answers with.
pub const RELEASE: &str = "8.19.22";

/// Where the fixture's server is installed, as its argv names it.
const HOME: &str = "usr/share/elasticsearch";

/// What `GET /` answers on 8.15.3, trimmed, with the release the fixture installs.
pub const ROOT: &str = r#"{
  "name" : "search-1",
  "cluster_name" : "docker-cluster",
  "cluster_uuid" : "uh7ULRBqQ1m4mIbk9MNkIg",
  "version" : {
    "number" : "8.19.22",
    "build_flavor" : "default",
    "build_type" : "docker",
    "build_hash" : "f97532e680b555c3a05e73a74c28afb666923018",
    "build_date" : "2024-10-09T22:08:00.328917561Z",
    "lucene_version" : "9.11.1"
  },
  "tagline" : "You Know, for Search"
}"#;

/// A plaintext listener that serves every connection with what `respond` makes of its request.
fn plain_listener(respond: impl Fn(&str) -> Vec<u8> + Send + 'static) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // A TLS record where HTTP was expected: measured on 8.19.22, the node answers five
            // bytes that are not TLS, an HTTP status line, and closes, logging nothing.
            let mut first = [0_u8; 1];
            if stream.peek(&mut first).is_ok_and(|read| read == 1) && first[0] == TLS_HANDSHAKE {
                let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
                continue;
            }
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                request.push(byte[0]);
            }
            let response = respond(&String::from_utf8_lossy(&request));
            let _ = stream.write_all(&response);
        }
    });
    port
}

/// A node answering `routes`, with a 404 for anything else, and the requests it was sent.
///
/// A path given more than once is answered with each in turn, the last kept for the rest.
pub struct FakeNode {
    pub port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    authorizations: Arc<Mutex<Vec<Option<String>>>>,
}

impl FakeNode {
    pub fn serving(routes: &[(&str, &str)]) -> Self {
        let answered: Vec<(&str, u16, &str)> = routes
            .iter()
            .map(|(path, body)| (*path, 200, *body))
            .collect();
        Self::answering(&answered)
    }

    /// A node answering each route with its own status, and a 404 for anything else.
    pub fn answering(routes: &[(&str, u16, &str)]) -> Self {
        Self::listening(routes, false, |_| {})
    }

    /// A node that runs `after` with each path it has answered, to change the box mid-read.
    pub fn serving_then(routes: &[(&str, &str)], after: impl Fn(&str) + Send + 'static) -> Self {
        let answered: Vec<(&str, u16, &str)> = routes
            .iter()
            .map(|(path, body)| (*path, 200, *body))
            .collect();
        Self::listening(&answered, false, after)
    }

    /// The same node on TLS, presenting a certificate nothing vouches for.
    pub fn serving_tls(routes: &[(&str, &str)]) -> Self {
        let answered: Vec<(&str, u16, &str)> = routes
            .iter()
            .map(|(path, body)| (*path, 200, *body))
            .collect();
        Self::listening(&answered, true, |_| {})
    }

    fn listening(
        routes: &[(&str, u16, &str)],
        tls: bool,
        after: impl Fn(&str) + Send + 'static,
    ) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let authorizations = Arc::new(Mutex::new(Vec::new()));
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(path, status, body)| ((*path).to_owned(), *status, (*body).to_owned()))
            .collect();
        let seen = Arc::clone(&requests);
        let authorized = Arc::clone(&authorizations);
        let respond = move |request: &str| {
            authorized
                .lock()
                .expect("the authorization log")
                .push(authorization_in(request));
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            let asked_before = {
                let mut seen = seen.lock().expect("the request log");
                let asked_before = seen.iter().filter(|asked| **asked == path).count();
                seen.push(path.clone());
                asked_before
            };
            after(&path);
            let answers: Vec<_> = routes
                .iter()
                .filter(|(route, _, _)| *route == path)
                .collect();
            match answers.get(asked_before).or(answers.last()) {
                Some((_, status, body)) => format!(
                    "HTTP/1.1 {status} Answer\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                    body.len()
                ),
                None => "HTTP/1.1 404 Not Found\r\ncontent-length: 2\r\n\r\n{}".to_owned(),
            }
            .into_bytes()
        };

        let port = match tls {
            true => super::tls_listener::serving(respond),
            false => plain_listener(respond),
        };
        Self {
            port,
            requests,
            authorizations,
        }
    }

    /// The `Authorization` header of each request so far, in order, `None` where it had none.
    pub fn authorizations(&self) -> Vec<Option<String>> {
        self.authorizations
            .lock()
            .expect("the authorization log")
            .clone()
    }

    /// The paths requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("the request log").clone()
    }

    /// A `/proc` holding one server whose HTTP listener is this node's, in rastro's namespace.
    ///
    /// `http.port` is pinned in the environment, because the listener's port is whatever the
    /// kernel gave the test and not one in the default range.
    pub fn proc(&self, name: &str) -> PathBuf {
        self.proc_with(name, &format!("http.port={}\0", self.port), None)
    }

    /// The same, with the environment and `elasticsearch.yml` the caller gives.
    pub fn proc_with(&self, name: &str, environ: &str, config_file: Option<&str>) -> PathBuf {
        let proc = scratch_tree(
            name,
            &[
                "self/ns",
                &format!("{PID}/fd"),
                &format!("{PID}/net"),
                &format!("{PID}/ns"),
                &format!("{PID}/root/etc/elasticsearch"),
            ],
        );
        write(&proc, &format!("{PID}/cmdline"), SERVER_ARGV);
        write(&proc, &format!("{PID}/environ"), environ);
        write(
            &proc,
            &format!("{PID}/net/tcp"),
            &format!(
                "{TCP_HEADER}   0: 0100007F:{:04X} 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 {SOCKET_INODE} 1 0000000000000000 100 0 0 10 0\n",
                self.port
            ),
        );
        symlink(
            format!("socket:[{SOCKET_INODE}]"),
            proc.join(PID).join("fd/3"),
        )
        .expect("a writable fixture");
        symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a writable fixture");
        symlink("net:[4026531840]", proc.join(PID).join("ns/net")).expect("a writable fixture");
        symlink("mnt:[4026531841]", proc.join("self/ns/mnt")).expect("a writable fixture");
        symlink("mnt:[4026531841]", proc.join(PID).join("ns/mnt")).expect("a writable fixture");

        // Every running node read a file at start, so the fixture always has one.
        write(
            &proc,
            &format!("{PID}/root/etc/elasticsearch/elasticsearch.yml"),
            config_file.unwrap_or(""),
        );
        super::process::started(&proc, PID, "1");
        install(&proc, RELEASE);

        proc
    }
}

fn authorization_in(request: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().to_owned())
    })
}

/// Puts `release`'s server jar in the fixture server's install, in place of any other.
pub fn install(proc: &std::path::Path, release: &str) {
    let lib = proc.join(PID).join("root").join(HOME).join("lib");
    let _ = std::fs::remove_dir_all(&lib);
    write(
        &proc.join(PID).join("root"),
        &format!("{HOME}/lib/elasticsearch-{release}.jar"),
        "",
    );
}
