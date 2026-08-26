//! Reading the telemetry fleet, without needing an exporter to run.
//!
//! Every fixture here is the development box's real output, quoted verbatim: six agents,
//! five of them Go binaries installed under `/usr/local/bin` by Ansible and one of them a
//! Debian package. That mix is the reason this facet exists, and it is measured rather than
//! supposed — `dpkg-query` knows `collectd 5.12.0-14` and has never heard of the other five.

use rastro::collectors::canonical_tool::ToolOutput;
use rastro::collectors::exporters::{
    ExporterBuild, ExportersCollector, TelemetryFleet, VersionDialect,
};
use rastro::collectors::systemd::UnitName;
use rastro_collector::{Collector, Content, Observation, Presence, Scalar};

/// The six agents as `systemctl show -p Id -p ExecStartEx --no-pager -- <units>` prints
/// them, alongside two units that are not telemetry at all.
const SHOWN: &str = "\
ExecStartEx={ path=/usr/local/bin/cadvisor ; argv[]=/usr/local/bin/cadvisor \
--store_container_labels=true --listen_ip=0.0.0.0 --port=8080 \
--prometheus_endpoint=/metrics --housekeeping_interval=10s ; flags= ; pid=0 }
Id=cadvisor.service

ExecStartEx={ path=/usr/local/bin/node_exporter ; argv[]=/usr/local/bin/node_exporter \
--collector.systemd --collector.textfile.directory=/var/lib/node_exporter \
--web.listen-address=0.0.0.0:9100 --web.telemetry-path=/metrics ; flags= ; pid=0 }
Id=node_exporter.service

ExecStartEx={ path=/usr/local/bin/process-exporter ; argv[]=/usr/local/bin/process-exporter \
--config.path=/etc/process_exporter/config.yml --web.listen-address=0.0.0.0:9256 ; \
flags= ; pid=0 }
Id=process_exporter.service

ExecStartEx={ path=/usr/local/bin/systemd_exporter ; argv[]=/usr/local/bin/systemd_exporter \
--systemd.collector.enable-restart-count --web.listen-address=0.0.0.0:9558 ; flags= ; pid=0 }
Id=systemd_exporter.service

ExecStartEx={ path=/usr/local/bin/postgres_exporter ; argv[]=/usr/local/bin/postgres_exporter \
--collector.stat_statements --web.listen-address=0.0.0.0:9187 \
--config.file=/etc/postgres_exporter/postgres_exporter.yml ; flags= ; pid=0 }
Id=postgres_exporter.service

ExecStartEx={ path=/usr/sbin/collectd ; argv[]=/usr/sbin/collectd ; flags= ; pid=0 }
Id=collectd.service

ExecStartEx={ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -D ; flags= ; pid=0 }
Id=ssh.service

Id=-.slice
";

fn deployment(unit: &str) -> rastro::collectors::exporters::Deployment {
    let name = UnitName::new(unit).expect("a legal unit name");

    TelemetryFleet::identify(SHOWN)
        .expect("these fixtures are well formed")
        .into_iter()
        .find(|deployment| deployment.unit == name)
        .unwrap_or_else(|| panic!("expected {unit:?} to be recognised as an exporter"))
}

fn field(observation: &Observation, name: &str) -> Observation {
    match observation.content() {
        Content::Object(entries) => entries
            .get(name)
            .unwrap_or_else(|| panic!("expected a {name:?} field"))
            .clone(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn integer(observation: &Observation) -> i64 {
    match observation.content() {
        Content::Scalar(Scalar::Integer(value)) => *value,
        other => panic!("expected an integer, got {other:?}"),
    }
}

fn keys_of(observation: &Observation) -> Vec<String> {
    match observation.content() {
        Content::Object(entries) => entries.keys().cloned().collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn on_stdout(printed: &str) -> ToolOutput {
    ToolOutput {
        stdout: printed.to_owned(),
        stderr: String::new(),
    }
}

fn on_stderr(printed: &str) -> ToolOutput {
    ToolOutput {
        stdout: String::new(),
        stderr: printed.to_owned(),
    }
}

#[test]
fn identify_finds_every_known_agent_and_nothing_else() {
    // Act
    let found = TelemetryFleet::identify(SHOWN).expect("these fixtures are well formed");

    // Assert: `ssh.service` runs a daemon and `-.slice` runs nothing, and neither is
    // telemetry. A facet that swept in every unit would be the units facet with extra steps.
    let units: Vec<&str> = found
        .iter()
        .map(|deployment| deployment.unit.as_str())
        .collect();
    assert_eq!(
        units,
        [
            "cadvisor.service",
            "collectd.service",
            "node_exporter.service",
            "postgres_exporter.service",
            "process_exporter.service",
            "systemd_exporter.service",
        ]
    );
}

#[test]
fn identify_recognises_an_agent_whose_unit_is_not_named_after_it() {
    // Act: `process_exporter.service` starts a binary called `process-exporter`, underscore
    // against hyphen, which is how that agent is really deployed.
    let deployment = deployment("process_exporter.service");

    // Assert: the binary is the identity and the unit name is incidental. Deriving one from
    // the other would miss this agent, and an operator free-naming their unit would be
    // missed the same way.
    assert_eq!(deployment.agent.as_str(), "process-exporter");
    assert_eq!(
        deployment.executable.as_str(),
        "/usr/local/bin/process-exporter"
    );
}

#[test]
fn identify_reads_the_endpoint_from_a_prometheus_listen_address() {
    // Act
    let endpoint = deployment("node_exporter.service")
        .endpoint
        .expect("this unit configures an address");

    // Assert
    let host = endpoint.host.expect("this address names one");
    assert_eq!(host.as_str(), "0.0.0.0");
    assert_eq!(endpoint.port.as_u16(), 9100);
    assert!(
        host.is_a_wildcard(),
        "which is what makes this agent reachable from off the box"
    );
}

#[test]
fn identify_reads_the_endpoint_cadvisor_spells_across_two_flags() {
    // Act: cAdvisor predates the Prometheus flag convention and takes `--listen_ip` and
    // `--port` separately.
    let endpoint = deployment("cadvisor.service")
        .endpoint
        .expect("this unit configures an address");

    // Assert: the same two fields as every other agent, which is the point of resolving the
    // dialects here rather than leaving six spellings in the document.
    assert_eq!(
        endpoint.host.expect("this unit names one").as_str(),
        "0.0.0.0"
    );
    assert_eq!(endpoint.port.as_u16(), 8080);
}

#[test]
fn identify_leaves_an_agent_without_a_configured_endpoint_absent() {
    // Act: collectd is started with no arguments at all. Its 9103 listener comes from the
    // `write_prometheus` plugin in `/etc/collectd/collectd.conf`, which is not argv.
    let deployment = deployment("collectd.service");

    // Assert: absent rather than invented. What is actually bound is the sockets facet's
    // answer, and the two disagreeing is a finding rather than a contradiction.
    assert!(deployment.endpoint.is_none());
    assert!(deployment.settings.is_empty());
}

#[test]
fn identify_keeps_the_flags_an_agent_was_started_with() {
    // Act
    let settings = deployment("postgres_exporter.service").settings;

    // Assert: keyed by flag name, so a diff names the flag that moved rather than reporting
    // one long argument string as changed.
    let named: Vec<&str> = settings.keys().map(|name| name.as_str()).collect();
    assert_eq!(
        named,
        [
            "collector.stat_statements",
            "config.file",
            "web.listen-address"
        ]
    );
    assert_eq!(
        settings
            .get(&rastro::collectors::exporters::SettingName::new("config.file").expect("legal"))
            .expect("this flag was given")
            .as_ref()
            .expect("it carries a value")
            .as_str(),
        "/etc/postgres_exporter/postgres_exporter.yml"
    );
}

#[test]
fn identify_records_a_boolean_flag_as_present_without_a_value() {
    // Act
    let settings = deployment("node_exporter.service").settings;

    // Assert: `--collector.systemd` turns a collector on by its bare presence. Recording it
    // as `true` would be rastro asserting a default it never observed.
    let systemd =
        rastro::collectors::exporters::SettingName::new("collector.systemd").expect("legal");
    assert!(
        settings
            .get(&systemd)
            .expect("this flag was given")
            .is_none()
    );
}

#[test]
fn a_prometheus_version_is_read_from_whichever_stream_carries_it() {
    // Arrange: node_exporter prints this to stdout and systemd_exporter prints the same
    // shape to stderr, both exiting zero. That split was measured on the box.
    let printed = "node_exporter, version 1.12.1 (branch: HEAD, revision: 6044da78)\n";

    // Act
    let from_stdout = VersionDialect::PrometheusCommon.parse(&on_stdout(printed));
    let from_stderr = VersionDialect::PrometheusCommon.parse(&on_stderr(printed));

    // Assert: an agent that answers on stderr is not an agent without a version.
    let expected = ExporterBuild {
        version: rastro::collectors::exporters::ExporterVersion::new("1.12.1").expect("legal"),
        revision: rastro::collectors::exporters::BuildRevision::new("6044da78").expect("legal"),
    };
    assert_eq!(from_stdout.expect("well formed"), Some(expected.clone()));
    assert_eq!(from_stderr.expect("well formed"), Some(expected));
}

#[test]
fn a_prometheus_version_survives_an_empty_branch() {
    // Arrange: process-exporter is built without a branch, so the field is empty. Real
    // output, and the kind of thing an invented fixture would never contain.
    let printed = "process-exporter, version 0.8.7 (branch: , revision: e52ab0a1)\n";

    // Act
    let build = VersionDialect::PrometheusCommon
        .parse(&on_stdout(printed))
        .expect("well formed")
        .expect("this agent reports a version");

    // Assert
    assert_eq!(build.version.as_str(), "0.8.7");
    assert_eq!(build.revision.as_str(), "e52ab0a1");
}

#[test]
fn cadvisor_spells_its_version_its_own_way() {
    // Act
    let build = VersionDialect::Cadvisor
        .parse(&on_stdout("cAdvisor version v0.49.2 (6876475a)\n"))
        .expect("well formed")
        .expect("this agent reports a version");

    // Assert: the revision is the half that moves when a rebuild of the same release lands,
    // which a version string alone would hide.
    assert_eq!(build.version.as_str(), "v0.49.2");
    assert_eq!(build.revision.as_str(), "6876475a");
}

#[test]
fn a_version_line_that_is_not_recognised_is_a_failure_rather_than_a_guess() {
    // Act
    let refused = VersionDialect::PrometheusCommon.parse(&on_stdout("v1.12.1\n"));

    // Assert: a shape rastro does not know is loud. Reading the first number-shaped token
    // out of any line at all would keep working when the binary is replaced by a wrapper
    // that prints something else entirely.
    assert!(refused.is_err());
}

#[test]
fn an_agent_with_no_version_flag_reports_no_build() {
    // Act: collectd's only version-ish flag is `-V`, which it rejects as invalid.
    let build = VersionDialect::None
        .parse(&on_stdout("Usage: collectd [OPTIONS]\n"))
        .expect("asking nothing cannot fail");

    // Assert: absent, and not a failure. collectd is a Debian package, so `dpkg-query`
    // already carries `5.12.0-14` in the packages facet; the five Go agents are the ones no
    // package manager has heard of, which is why this facet reads versions at all.
    assert!(build.is_none());
}

#[test]
fn the_facet_is_absent_when_no_known_agent_is_deployed() {
    // Arrange: a box that runs an ssh daemon and nothing telemetric.
    let shown = "ExecStartEx={ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -D ; flags= ; \
pid=0 }\nId=ssh.service\n";

    // Act
    let found = TelemetryFleet::identify(shown).expect("well formed");

    // Assert
    assert!(found.is_empty());
}

#[test]
fn presence_is_absent_without_systemd() {
    // Act & Assert: every agent here is dispatched from a systemd unit, so a box with no
    // systemd is one where rastro has not looked rather than one it can call empty.
    assert_eq!(
        ExportersCollector::reading(None).presence(),
        Presence::Absent
    );
}

#[test]
fn the_observation_is_keyed_by_unit_so_two_instances_of_one_agent_both_appear() {
    // Arrange: two postgres_exporters, one per cluster, which is how a box with two
    // PostgreSQL versions is really monitored.
    let shown = "\
ExecStartEx={ path=/usr/local/bin/postgres_exporter ; \
argv[]=/usr/local/bin/postgres_exporter --web.listen-address=0.0.0.0:9187 ; flags= ; pid=0 }
Id=postgres_exporter@15.service

ExecStartEx={ path=/usr/local/bin/postgres_exporter ; \
argv[]=/usr/local/bin/postgres_exporter --web.listen-address=0.0.0.0:9188 ; flags= ; pid=0 }
Id=postgres_exporter@16.service
";

    // Act
    let found = TelemetryFleet::identify(shown).expect("well formed");
    let observation = Observation::from(&TelemetryFleet::fleet_of(found));

    // Assert: keyed by unit, not by agent, so the second instance is a second entry rather
    // than one silently overwriting the other.
    assert_eq!(
        keys_of(&observation),
        [
            "postgres_exporter@15.service",
            "postgres_exporter@16.service"
        ]
    );
    assert_eq!(
        integer(&field(
            &field(
                &field(&observation, "postgres_exporter@16.service"),
                "endpoint"
            ),
            "port"
        )),
        9188
    );
}

#[test]
fn the_observation_carries_the_agent_its_endpoint_and_its_flags() {
    // Act
    let found = TelemetryFleet::identify(SHOWN).expect("well formed");
    let observation = Observation::from(&TelemetryFleet::fleet_of(found));
    let node = field(&observation, "node_exporter.service");

    // Assert
    assert_eq!(
        keys_of(&node),
        ["agent", "build", "endpoint", "executable", "settings"]
    );
    assert_eq!(text(&field(&node, "agent")), "node_exporter");
    assert_eq!(text(&field(&field(&node, "endpoint"), "host")), "0.0.0.0");
    assert_eq!(integer(&field(&field(&node, "endpoint"), "port")), 9100);
    assert_eq!(
        text(&field(&field(&node, "settings"), "web.telemetry-path")),
        "/metrics"
    );
}

#[test]
fn an_agent_with_nothing_to_report_renders_its_absences_as_null() {
    // Act
    let found = TelemetryFleet::identify(SHOWN).expect("well formed");
    let observation = Observation::from(&TelemetryFleet::fleet_of(found));
    let collectd = field(&observation, "collectd.service");

    // Assert: the keys are always the same set, because a key that is sometimes there and
    // sometimes not is awkward for every consumer of the document.
    assert_eq!(
        field(&collectd, "endpoint").content(),
        &Content::Scalar(Scalar::Null)
    );
    assert_eq!(
        field(&collectd, "build").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn nothing_in_the_facet_is_volatile() {
    // Act
    let found = TelemetryFleet::identify(SHOWN).expect("well formed");
    let observation = Observation::from(&TelemetryFleet::fleet_of(found));

    // Assert: a version, an endpoint and a set of flags are all stable across two runs of an
    // unchanged box. What these agents *measure* changes by the second, and this facet is
    // deliberately not where any of it lives.
    assert_eq!(
        observation.in_view(rastro_fingerprint::View::Diffable),
        Some(observation.clone())
    );
}

#[test]
fn an_address_that_omits_its_host_keeps_the_port_and_no_host() {
    // Arrange: `:9100` is the idiomatic Go spelling for every interface, and a real one.
    let shown = "ExecStartEx={ path=/usr/local/bin/node_exporter ; \
argv[]=/usr/local/bin/node_exporter --web.listen-address=:9100 ; flags= ; pid=0 }\n\
Id=node_exporter.service\n";

    // Act
    let endpoint = TelemetryFleet::identify(shown)
        .expect("well formed")
        .remove(0)
        .endpoint
        .expect("a port is an endpoint");

    // Assert: absent, not widened to `0.0.0.0`. Go binds every family for an empty host, so
    // writing an IPv4 wildcard in its place would assert a narrower bind than was asked for.
    assert!(endpoint.host.is_none());
    assert_eq!(endpoint.port.as_u16(), 9100);
}

#[test]
fn an_ipv6_address_loses_only_its_brackets() {
    // Arrange
    let shown = "ExecStartEx={ path=/usr/local/bin/node_exporter ; \
argv[]=/usr/local/bin/node_exporter --web.listen-address=[::]:9100 ; flags= ; pid=0 }\n\
Id=node_exporter.service\n";

    // Act
    let endpoint = TelemetryFleet::identify(shown)
        .expect("well formed")
        .remove(0)
        .endpoint
        .expect("this address names a host");

    // Assert: split from the right, because an IPv6 address is full of colons and only the
    // last one separates the port. The brackets are that split's punctuation, and the
    // sockets facet strips them too, which is what lets the two be compared.
    assert_eq!(endpoint.host.expect("named").as_str(), "::");
    assert_eq!(endpoint.port.as_u16(), 9100);
}

#[test]
fn an_argument_that_is_not_a_flag_is_refused() {
    // Arrange: a value containing a space, which systemd renders indistinguishably from two
    // arguments. `/etc/my dir/c.yml` arrives as two tokens and the second is not a flag.
    let shown = "ExecStartEx={ path=/usr/local/bin/postgres_exporter ; \
argv[]=/usr/local/bin/postgres_exporter --config.file=/etc/my dir/c.yml ; flags= ; pid=0 }\n\
Id=postgres_exporter.service\n";

    // Act
    let refused = TelemetryFleet::identify(shown);

    // Assert: loud rather than a settings map that quietly dropped half a path. This is the
    // refutable-bad-split rule that lets this facet tokenise argv where the units facet
    // deliberately does not.
    assert!(refused.is_err());
}
