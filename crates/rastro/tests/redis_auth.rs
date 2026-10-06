//! Reaching a server that wants a password, and never sending one it does not.
//!
//! **Every `AUTH` sent is a fact about the host.** A failed one is an entry in the server's
//! `ACL LOG`, measured on Debian 12, so these tests assert what went over the wire as well as
//! what the document says: one attempt at most, only after `NOAUTH`, with the password the
//! server was started with.

use std::fs;
use std::path::PathBuf;

use rastro::collectors::redis::{InstalledServers, RedisCollector};
use rastro_collector::{Collector, Observation};

mod support;

use support::fake_redis::{FakeRedis, key_of, server_pid};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, keys_of, text};
use support::shim;

/// A box: the fake server's `/proc`, a unit, and a configuration file for it.
struct AuthBox {
    proc: PathBuf,
    config: PathBuf,
    systemctl_log: PathBuf,
    bin: PathBuf,
}

/// Arrange: the Debian unit's own `ExecStart`, a file first and flags after it, which is how
/// redis tells a configuration file from an option.
fn auth_box(name: &str, server: &FakeRedis, argv_tail: &str) -> AuthBox {
    let root = scratch_tree(name, &["bin", "etc"]);
    let proc = server.proc(&format!("{name}-proc"));
    let config = root.join("etc/redis.conf");

    AuthBox {
        proc,
        systemctl_log: root.join("systemctl.log"),
        bin: root.join("bin"),
        config: config.clone(),
    }
    .with_unit(&format!("{} {argv_tail}", config.display()))
}

impl AuthBox {
    /// A `systemctl` that shows the unit starting the server with `arguments`, and records how
    /// it was asked.
    fn with_unit(self, arguments: &str) -> Self {
        self.with_unit_running("/usr/bin/redis-server", arguments)
    }

    /// A unit whose start command runs `program`, which need not be a redis server.
    fn with_unit_running(self, program: &str, arguments: &str) -> Self {
        let dump = format!(
            "ExecStartEx={{ path={program} ; argv[]={program} {arguments} ; flags= ; pid=0 }}\nId=redis-server.service\n"
        );
        fs::write(self.bin.join("dump"), dump).expect("a writable fixture");

        self.with_control_group("/system.slice/redis-server.service")
    }

    /// The cgroup systemd says the unit runs in, which the server's own must match.
    fn with_control_group(self, path: &str) -> Self {
        fs::write(self.bin.join("control_group"), format!("{path}\n")).expect("a writable fixture");

        self
    }

    fn collector(&self) -> RedisCollector {
        let systemctl = shim::executable(
            &self.bin,
            "systemctl",
            &format!(
                "#!/bin/sh\necho \"$@\" >> {log}\ncase \"$*\" in\n  *--property=ControlGroup*) cat {group} ;;\n  *) cat {dump} ;;\nesac\n",
                log = self.systemctl_log.display(),
                dump = self.bin.join("dump").display(),
                group = self.bin.join("control_group").display()
            ),
        );

        RedisCollector::reading(InstalledServers::new([]), &self.proc).asking_systemd(systemctl)
    }

    fn configured(self, contents: &str) -> Self {
        write(
            self.config.parent().expect("a parent"),
            "redis.conf",
            contents,
        );
        self
    }

    fn in_cgroup(self, line: &str) -> Self {
        write(&self.proc, &format!("{}/cgroup", server_pid()), line);
        self
    }

    fn systemctl_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.systemctl_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn instance(observation: &Observation, server: &FakeRedis) -> Observation {
    let instances = field(observation, "instances");
    assert_eq!(keys_of(&instances), [key_of(server)]);

    field(&instances, &key_of(server))
}

/// The `AUTH` commands the server received, as their arguments.
fn auths(server: &FakeRedis) -> Vec<Vec<String>> {
    server
        .received()
        .into_iter()
        .filter(|words| words[0] == "AUTH")
        .collect()
}

fn read(auth_box: &AuthBox) -> Observation {
    auth_box.collector().collect().expect("a readable box")
}

#[test]
fn the_password_the_servers_own_file_sets_unlocks_it() {
    // Arrange
    let server = FakeRedis::stock_with_password("auth-file", "hunter2", &[]);
    let auth_box = auth_box(
        "redis-auth-file",
        &server,
        "--supervised systemd --daemonize no",
    )
    .configured("bind 127.0.0.1 -::1\nrequirepass hunter2\n");

    // Act
    let observation = read(&auth_box);

    // Assert: read in full, after exactly one `AUTH` that followed the refusal.
    let instance = instance(&observation, &server);
    assert!(is_null(&field(&instance, "error")), "{instance:?}");
    assert_eq!(text(&field(&instance, "version")), "7.0.15");
    assert_eq!(auths(&server), [["AUTH", "hunter2"]]);
    assert_eq!(server.received()[0], ["INFO", "server"]);
}

#[test]
fn a_server_that_answers_without_a_password_is_never_sent_one() {
    // Arrange: the file sets a password the running server does not ask for, which is what a
    // `CONFIG SET requirepass ""` leaves behind.
    let server = FakeRedis::stock("auth-open", &[]);
    let auth_box = auth_box("redis-auth-open", &server, "").configured("requirepass hunter2\n");

    // Act
    let observation = read(&auth_box);

    // Assert: and systemd was not even asked, because nothing needed a password.
    assert!(is_null(&field(&instance(&observation, &server), "error")));
    assert!(auths(&server).is_empty());
    assert!(auth_box.systemctl_calls().is_empty());
}

#[test]
fn a_quoted_password_is_read_the_way_redis_reads_it() {
    // Arrange
    let server = FakeRedis::stock_with_password("auth-quoted", "two \"words\"", &[]);
    let auth_box = auth_box("redis-auth-quoted", &server, "")
        .configured("requirepass \"two \\\"words\\\"\"\n");

    // Act
    read(&auth_box);

    // Assert
    assert_eq!(auths(&server), [["AUTH", "two \"words\""]]);
}

#[test]
fn a_password_in_an_included_file_is_found_and_the_last_one_wins() {
    // Arrange: the file sets one, then includes a file that sets another, which is the one the
    // server read last and so the one it runs with.
    let server = FakeRedis::stock_with_password("auth-include", "newer", &[]);
    let auth_box = auth_box("redis-auth-include", &server, "");
    let included = auth_box.config.with_file_name("local.conf");
    fs::write(&included, "requirepass newer\n").expect("a writable fixture");
    let auth_box = auth_box.configured(&format!(
        "requirepass older\ninclude {}\n",
        included.display()
    ));

    // Act
    read(&auth_box);

    // Assert
    assert_eq!(auths(&server), [["AUTH", "newer"]]);
}

#[test]
fn a_password_on_the_command_line_outranks_the_file() {
    // Arrange: redis applies its arguments after the file.
    let server = FakeRedis::stock_with_password("auth-argv", "fromargv", &[]);
    let auth_box = auth_box("redis-auth-argv", &server, "--requirepass fromargv")
        .configured("requirepass fromfile\n");

    // Act
    read(&auth_box);

    // Assert
    assert_eq!(auths(&server), [["AUTH", "fromargv"]]);
}

#[test]
fn a_password_set_only_at_runtime_is_said_to_be_unreachable() {
    // Arrange: the field host's arrangement, `CONFIG SET requirepass` and no line in any file.
    let server = FakeRedis::stock_with_password("auth-runtime", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-runtime", &server, "").configured("bind 0.0.0.0\n");

    // Act
    let observation = read(&auth_box);

    // Assert: said, and nothing guessed at.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("sets no password"), "{error}");
    assert!(auths(&server).is_empty());
}

#[test]
fn a_password_the_server_refuses_is_sent_once_and_named_as_a_disagreement() {
    // Arrange: rotated at runtime since the file was written.
    let server = FakeRedis::stock_with_password("auth-stale", "rotated", &[]);
    let auth_box =
        auth_box("redis-auth-stale", &server, "").configured("requirepass previous-password\n");

    // Act
    let observation = read(&auth_box);

    // Assert: never retried, since each refusal is an entry in the server's `ACL LOG`, and
    // the password itself is nowhere in the document.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("refused"), "{error}");
    assert!(!error.contains("previous-password"), "{error}");
    assert_eq!(auths(&server), [["AUTH", "previous-password"]]);
    assert_eq!(server.received().last().expect("a command")[0], "AUTH");
}

#[test]
fn a_server_no_unit_started_is_not_guessed_at() {
    // Arrange: started by hand, in somebody's login session.
    let server = FakeRedis::stock_with_password("auth-no-unit", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-no-unit", &server, "")
        .configured("requirepass hunter2\n")
        .in_cgroup("0::/user.slice/user-1000.slice/session-3.scope\n");

    // Act
    let observation = read(&auth_box);

    // Assert: `/etc/redis/redis.conf` would be a guess, and a wrong guess is an `ACL LOG` entry.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("unit"), "{error}");
    assert!(auths(&server).is_empty());
    assert!(auth_box.systemctl_calls().is_empty());
}

#[test]
fn a_unit_name_that_reads_as_an_option_is_never_handed_to_systemctl() {
    // Arrange
    let server = FakeRedis::stock_with_password("auth-option-unit", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-option-unit", &server, "")
        .in_cgroup("0::/system.slice/--kill.service\n");

    // Act
    let observation = read(&auth_box);

    // Assert
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("unit"), "{error}");
    assert!(auth_box.systemctl_calls().is_empty());
}

#[test]
fn systemd_is_asked_about_the_servers_own_unit_and_nothing_else() {
    // Arrange: a template instance, whose cgroup nests it under its template's slice.
    let server = FakeRedis::stock_with_password("auth-template", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-template", &server, "")
        .configured("requirepass hunter2\n")
        .in_cgroup("0::/system.slice/system-redis\\x2dserver.slice/redis-server@cache.service\n")
        .with_control_group(
            "/system.slice/system-redis\\x2dserver.slice/redis-server@cache.service",
        );

    // Act
    read(&auth_box);

    // Assert
    let calls = auth_box.systemctl_calls();
    assert!(!calls.is_empty());
    assert!(
        calls
            .iter()
            .all(|call| call.ends_with("-- redis-server@cache.service")),
        "{calls:?}"
    );
}

#[test]
fn a_file_that_cannot_be_read_says_which_and_why() {
    // Arrange: the unit names a file that is not there.
    let server = FakeRedis::stock_with_password("auth-no-file", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-no-file", &server, "");

    // Act
    let observation = read(&auth_box);

    // Assert
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(
        error.contains(&auth_box.config.display().to_string()),
        "{error}"
    );
    assert!(auths(&server).is_empty());
}

#[test]
fn a_unit_that_starts_the_server_with_options_alone_names_no_file() {
    // Arrange: redis takes its first argument as the file only when it is not an option.
    let server = FakeRedis::stock_with_password("auth-options-only", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-options-only", &server, "").with_unit("--port 6379");

    // Act
    let observation = read(&auth_box);

    // Assert
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("no configuration file"), "{error}");
    assert!(auths(&server).is_empty());
}

/// `sha256("hunter2")`, the verifier `CONFIG REWRITE` writes for it.
const HUNTER2_HASH: &str = "f52fbd32b2b3b86ff88ef6c490628285f482af15ddcb29541f94bcf526a3f6c7";

#[test]
fn a_file_config_rewrite_left_behind_is_read_through_its_account_line() {
    // Arrange: measured, `CONFIG REWRITE` keeps `requirepass` and appends this line.
    let server = FakeRedis::stock_with_password("auth-rewritten", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-rewritten", &server, "").configured(&format!(
        "requirepass \"hunter2\"\n# Generated by CONFIG REWRITE\nuser default on #{HUNTER2_HASH} ~* &* +@all\n"
    ));

    // Act
    let observation = read(&auth_box);

    // Assert
    assert!(is_null(&field(&instance(&observation, &server), "error")));
    assert_eq!(auths(&server), [["AUTH", "hunter2"]]);
}

#[test]
fn a_requirepass_edited_after_a_rewrite_is_never_sent() {
    // Arrange: the hand-written line changed later, the generated one left alone, so the
    // account still holds the old verifier and the file's `requirepass` opens nothing.
    let server = FakeRedis::stock_with_password("auth-drifted", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-drifted", &server, "").configured(&format!(
        "requirepass edited-password\nuser default on #{HUNTER2_HASH} ~* &* +@all\n"
    ));

    // Act
    let observation = read(&auth_box);

    // Assert: said, and no `ACL LOG` entry made to find out.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("hash"), "{error}");
    assert!(!error.contains("edited-password"), "{error}");
    assert!(auths(&server).is_empty());
}

#[test]
fn an_acl_file_outranks_requirepass() {
    // Arrange: measured, the server ignores `requirepass` once an ACL file is named.
    let server = FakeRedis::stock_with_password("auth-aclfile", "fromacl", &[]);
    let auth_box = auth_box("redis-auth-aclfile", &server, "");
    let acl_file = auth_box.config.with_file_name("users.acl");
    fs::write(&acl_file, "user default on >fromacl ~* &* +@all\n").expect("a writable fixture");
    let auth_box = auth_box.configured(&format!(
        "requirepass fromconf\naclfile {}\n",
        acl_file.display()
    ));

    // Act
    read(&auth_box);

    // Assert
    assert_eq!(auths(&server), [["AUTH", "fromacl"]]);
}

#[test]
fn an_acl_file_without_the_default_account_is_a_disagreement() {
    // Arrange: measured, such a server answers without a password, so this one is not it.
    let server = FakeRedis::stock_with_password("auth-acl-nodefault", "fromconf", &[]);
    let auth_box = auth_box("redis-auth-acl-nodefault", &server, "");
    let acl_file = auth_box.config.with_file_name("users.acl");
    fs::write(&acl_file, "user alice on >a ~* +@all\n").expect("a writable fixture");
    let auth_box = auth_box.configured(&format!(
        "requirepass fromconf\naclfile {}\n",
        acl_file.display()
    ));

    // Act
    let observation = read(&auth_box);

    // Assert
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("without a password"), "{error}");
    assert!(auths(&server).is_empty());
}

#[test]
fn a_refused_password_is_named_as_changed_since_the_server_started() {
    // Arrange: a file that agrees with itself, and a server rotated at runtime since.
    let server = FakeRedis::stock_with_password("auth-rotated", "rotated", &[]);
    let auth_box =
        auth_box("redis-auth-rotated", &server, "").configured("requirepass old-password\n");

    // Act
    let observation = read(&auth_box);

    // Assert: the file is consistent, so the only thing left to disagree is the running server.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(
        error.contains("changed since the server started"),
        "{error}"
    );
}

#[test]
fn a_unit_that_starts_something_else_is_not_taken_for_the_servers() {
    // Arrange: measured on GitHub's runner, a redis started by a job runs in the job agent's
    // cgroup, `hosted-compute-agent.service`, so the enclosing unit is not the one that
    // started redis; a cron job or another service's script does the same.
    let server = FakeRedis::stock_with_password("auth-foreign-unit", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-foreign-unit", &server, "")
        .configured("requirepass hunter2\n")
        .in_cgroup("0::/system.slice/hosted-compute-agent.service\n");
    let config = auth_box.config.display().to_string();
    let auth_box = auth_box
        .with_unit_running("/opt/runner/agent", &config)
        .with_control_group("/system.slice/hosted-compute-agent.service");

    // Act
    let observation = read(&auth_box);

    // Assert: the file the other unit names is never read as redis's, nothing is sent, and
    // the reason names the unit.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("hosted-compute-agent.service"), "{error}");
    assert!(error.contains("does not start a redis server"), "{error}");
    assert!(auths(&server).is_empty());
}

#[test]
fn a_users_own_unit_sharing_a_system_units_name_is_not_taken_for_it() {
    // Arrange: a redis under the user manager, in a unit the user called `redis-server.service`.
    // The last component of its cgroup names the system unit too, whose file holds the system
    // server's password.
    let server = FakeRedis::stock_with_password("auth-user-unit", "hunter2", &[]);
    let auth_box = auth_box("redis-auth-user-unit", &server, "")
        .configured("requirepass hunter2\n")
        .in_cgroup(
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/redis-server.service\n",
        );

    // Act
    let observation = read(&auth_box);

    // Assert: systemd's own cgroup for the unit is not the server's, so its file is not read.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(
        error.contains("does not run in redis-server.service"),
        "{error}"
    );
    assert!(auths(&server).is_empty());
}

#[test]
fn a_command_line_option_that_changes_the_account_is_refused_rather_than_ignored() {
    // Arrange: redis applies every `--option value` after the file, so `--aclfile` here would
    // decide the password, and rastro does not replay it.
    let server = FakeRedis::stock_with_password("auth-argv-aclfile", "hunter2", &[]);
    let auth_box = auth_box(
        "redis-auth-argv-aclfile",
        &server,
        "--aclfile /etc/redis/users.acl",
    )
    .configured("requirepass hunter2\n");

    // Act
    let observation = read(&auth_box);

    // Assert
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("--aclfile"), "{error}");
    assert!(auths(&server).is_empty());
}

#[test]
fn a_command_line_password_systemd_shows_ambiguously_is_not_sent() {
    // Arrange: systemd prints the vector without its quoting, so `--requirepass two words`
    // cannot be told from a password `two` followed by a word that is not an option.
    let server = FakeRedis::stock_with_password("auth-argv-spaced", "two words", &[]);
    let auth_box = auth_box("redis-auth-argv-spaced", &server, "--requirepass two words");

    // Act
    let observation = read(&auth_box);

    // Assert: a cut password is a wrong one, and a wrong one is an `ACL LOG` entry.
    let error = text(&field(&instance(&observation, &server), "error"));
    assert!(error.contains("--requirepass"), "{error}");
    assert!(auths(&server).is_empty());
}
