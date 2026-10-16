//! rastro's reading of a live server against the server's own answers.
//!
//! Every other redis test asserts what rastro does with a reply somebody measured once, which
//! pins the code and cannot catch a reply that was written down wrong. This one starts a real
//! `redis-server`, reads it through the real `/proc`, and compares with what the server and its
//! own client say.
//!
//! **It fails rather than skipping where there is no `redis-server`**, which is the
//! `nginx_conformance` rule and for its reason: a check that quietly passes on a box without the
//! service is how three earlier defects hid. `scripts/container-suite.sh` installs it.
//!
//! The server is contained: a loopback port nothing else holds, persistence off, its directory
//! inside this test's own scratch tree, and killed when the test ends. It runs as whoever runs the
//! suite, so the unprivileged half of the container suite reads a server it owns.

use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use rastro::collectors::redis::RedisCollector;
use rastro_collector::{ClaimedReading, Collector, Observation};
use rastro_fingerprint::Sensitivity;

mod support;

use support::fs_tree::scratch_tree;
use support::observation::{field, is_null, keys_of, text};

const SERVER: &str = "redis-server";
const CLIENT: &str = "redis-cli";

/// How long a server gets to start listening.
const START_WITHIN: Duration = Duration::from_secs(10);

/// A server this test started, stopped when the test ends whatever the outcome.
struct LiveServer {
    child: Child,
    port: u16,
    directory: PathBuf,
}

impl LiveServer {
    fn start(name: &str, extra: &[&str]) -> Self {
        let directory = scratch_tree(name, &["data"]).join("data");
        // A port the kernel just handed out and took back, which nothing else is likely to take.
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a loopback port")
            .port();
        let port_text = port.to_string();

        let mut arguments = vec![
            "--port",
            &port_text,
            "--bind",
            "127.0.0.1",
            "--dir",
            directory.to_str().expect("a UTF-8 path"),
            "--save",
            "",
            "--appendonly",
            "no",
            "--daemonize",
            "no",
            "--loglevel",
            "warning",
        ];
        arguments.extend_from_slice(extra);

        let child = Command::new(SERVER)
            .args(&arguments)
            .spawn()
            .unwrap_or_else(|error| {
                panic!(
                    "{SERVER} could not be started ({error}); this check fails rather than \
                     skipping, so install redis to run the suite"
                )
            });
        let server = Self {
            child,
            port,
            directory,
        };

        let deadline = Instant::now() + START_WITHIN;
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "{SERVER} never listened");
            thread::sleep(Duration::from_millis(20));
        }

        server
    }

    fn key(&self) -> String {
        self.port.to_string()
    }

    /// What the server's own client prints for a command, one line per reply element.
    fn client(&self, command: &[&str]) -> Vec<String> {
        let output = Command::new(CLIENT)
            .args(["-h", "127.0.0.1", "-p"])
            .arg(self.port.to_string())
            .args(command)
            .output()
            .expect("the server's own client");
        assert!(output.status.success(), "{CLIENT} failed: {output:?}");

        String::from_utf8(output.stdout)
            .expect("UTF-8")
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The collector as a run builds it.
fn collector() -> RedisCollector {
    RedisCollector::new()
}

/// The instance this test's server is, among whatever else runs on the box.
fn instance_of(server: &LiveServer) -> Observation {
    let observation = collector().collect().expect("a readable box");
    let instances = field(&observation, "instances");
    assert!(
        keys_of(&instances).contains(&server.key()),
        "no instance for {}: {:?}",
        server.key(),
        keys_of(&instances)
    );

    field(&instances, &server.key())
}

#[test]
fn a_live_server_is_read_the_way_it_reports_itself() {
    // Arrange
    let server = LiveServer::start("redis-live-read", &[]);
    let version = Command::new(SERVER)
        .arg("--version")
        .output()
        .expect("the server's version");
    let version = String::from_utf8(version.stdout).expect("UTF-8");

    // Act
    let instance = instance_of(&server);

    // Assert: nothing missing, and every setting the server's own client lists is here.
    assert!(is_null(&field(&instance, "error")), "{instance:?}");
    let reported = text(&field(&instance, "version"));
    assert!(
        version.contains(&format!("v={reported} ")),
        "{reported} against {version}"
    );
    let settings = field(&instance, "settings");
    assert_eq!(
        keys_of(&settings).len() * 2,
        server.client(&["CONFIG", "GET", "*"]).len()
    );
    assert_eq!(
        text(&field(&settings, "dir")),
        server.directory.to_str().expect("a UTF-8 path")
    );
    assert_eq!(text(&field(&settings, "save")), "");
}

#[test]
fn a_live_servers_password_is_carried_sensitive() {
    // Arrange
    let server = LiveServer::start("redis-live-secret", &[]);
    server.client(&["CONFIG", "SET", "masterauth", "masterauth-password"]);

    // Act
    let settings = field(&instance_of(&server), "settings");

    // Assert
    let masterauth = field(&settings, "masterauth");
    assert_eq!(masterauth.sensitivity(), Sensitivity::Sensitive);
    assert_eq!(text(&masterauth), "masterauth-password");
}

#[test]
fn a_live_server_reads_the_same_twice() {
    // Arrange
    let server = LiveServer::start("redis-live-twice", &[]);

    // Act
    let first = instance_of(&server);
    let second = instance_of(&server);

    // Assert: the facet's whole claim, on a real server rather than a script.
    assert_eq!(first, second);
}

#[test]
fn a_live_server_wanting_a_password_is_said_so() {
    // Arrange
    let server = LiveServer::start(
        "redis-live-noauth",
        &["--requirepass", "unreached-password"],
    );

    // Act
    let instance = instance_of(&server);

    // Assert
    let reason = text(&field(&instance, "not_read"));
    assert!(reason.contains("requires a password"), "{reason}");
    assert!(!reason.contains("unreached-password"), "{reason}");
}

#[test]
fn a_live_servers_own_files_are_the_ones_claimed() {
    // Arrange
    let server = LiveServer::start("redis-live-claim", &[]);

    // Act
    let claims = collector().filesystem_claims();

    // Assert: in the process's working directory, which is what `dir` sets.
    let mut trees: Vec<&str> = claims
        .iter()
        .filter(|claim| {
            claim.qualifier().map(|qualifier| qualifier.as_str()) == Some(&server.key())
        })
        .map(|claim| {
            assert_eq!(claim.reading(), ClaimedReading::Sealed);
            claim.tree().as_str()
        })
        .collect();
    trees.sort_unstable();
    let directory = server.directory.to_str().expect("a UTF-8 path");
    assert_eq!(
        trees,
        [
            format!("{directory}/appendonly.aof"),
            format!("{directory}/appendonlydir"),
            format!("{directory}/dump.rdb")
        ]
    );
}
