//! The `containers` facet: which container engines are on the box, and what each is running.
//!
//! The fixtures are real output, captured from docker 29.8.0 with containerd 2.3.4 under it,
//! and trimmed to the fields this facet reads plus a few it deliberately ignores. Trimmed
//! rather than invented: a fixture written from memory tests rastro against the author's
//! recollection of docker, which is the mistake the nginx grammar entry in
//! `docs/decisions.md` records paying for.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::ContainersCollector;
use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::containers::{Docker, EngineSource};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Observation, Sensitivity, Volatility};
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, integer, is_null, items_of, keys_of, text};

/// `docker version --format '{{json .}}'` on a box whose daemon answers.
///
/// The components are the reason this probe is worth reading rather than only being a
/// liveness check: they say which containerd and which runc the engine actually runs, and a
/// runc upgrade is exactly the change a fingerprint is taken to catch.
const VERSION_ANSWERING: &str = r#"{
  "Client": { "Version": "29.8.0", "ApiVersion": "1.56", "Context": "default" },
  "Server": {
    "Platform": { "Name": "Docker Engine - Community" },
    "Version": "29.8.0",
    "ApiVersion": "1.56",
    "MinAPIVersion": "1.40",
    "Components": [
      { "Name": "Engine", "Version": "29.8.0", "Details": { "GitCommit": "3ce5872" } },
      { "Name": "containerd", "Version": "v2.3.4", "Details": { "GitCommit": "db88095" } },
      { "Name": "runc", "Version": "1.5.1", "Details": { "GitCommit": "v1.5.1-0-g8f2685a" } },
      { "Name": "docker-init", "Version": "0.19.0", "Details": { "GitCommit": "de40ad0" } }
    ],
    "KernelVersion": "7.1.3-200.fc44.aarch64"
  }
}"#;

/// The same probe on a box where docker is installed and its daemon is not answering.
///
/// **Measured on docker 29.8.0, because it decides the whole detection ladder.** `docker
/// version` exits *zero* here, prints the client half on stdout with `"Server": null`, and
/// writes the connection failure to stderr. `docker info` and `docker ps` exit 1 in the same
/// situation, which is why this is the probe and neither of those is.
///
/// Docker 26.1.5 exits 1 for the same read, so this state is only reachable on a newer
/// client; the older one produces a facet `error` instead. Both are recorded in
/// `docs/decisions.md`, and the shape is what podman and containerd will report through.
const VERSION_UNREACHABLE: &str = r#"{
  "Client": { "Version": "29.8.0", "ApiVersion": "1.56", "Context": "default" },
  "Server": null
}"#;

const UNREACHABLE_STDERR: &str = "failed to connect to the docker API at \
unix:///var/run/docker.sock; check if the path is correct and if the daemon is running: \
dial unix /var/run/docker.sock: connect: no such file or directory";

/// `docker info --format '{{json .}}'`, trimmed.
///
/// `Containers`, `Images` and `NCPU` are kept deliberately: they are counts this facet does
/// not read, because the container list is the answer and the CPU count is the host's
/// business, and a fixture that dropped them could not catch a reader that started using
/// them.
const INFO_ANSWERING: &str = r#"{
  "ID": "d94030bd-2938-4d0a-9d0e-000000000000",
  "Containers": 1,
  "Images": 2,
  "NCPU": 7,
  "Driver": "overlayfs",
  "DockerRootDir": "/var/lib/docker",
  "CgroupDriver": "cgroupfs",
  "CgroupVersion": "2",
  "LoggingDriver": "json-file",
  "DefaultRuntime": "runc",
  "LiveRestoreEnabled": false,
  "SecurityOptions": ["name=seccomp,profile=builtin", "name=cgroupns"],
  "Swarm": { "NodeID": "", "LocalNodeState": "inactive", "ControlAvailable": false },
  "ServerVersion": "29.8.0"
}"#;

const WEB_ID: &str = "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e";
const STOPPED_ID: &str = "551e41b5515383f95ae02559fc0a1cd7500d88be62c2add2d7a2cfdc3746ac5a";
const EPHEMERAL_ID: &str = "1db8c55268930421b2a804afd47b84b7ff88c7ee942242c529431fef314c5ea6";
const LIMITED_ID: &str = "3c1f6c1f9f5f4a1d8e2b7c6d5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b";
const BRIDGE_NETWORK_ID: &str = "f6267b7bcdf233149bcaccaeb877e708696b17af2347660ce623c8397e4bd1fd";
const FIXTURE_NETWORK_ID: &str = "1d51e8c10dda55fd904537227940fe186820f04574b31a6a924f628f8f7dcb13";
const TAGGED_IMAGE: &str =
    "sha256:fa10ef3b6224d65632b644294314ddefb5b9185fb87ba29e9bd8d8c2ba86dc02";
const DANGLING_IMAGE: &str =
    "sha256:f736818d54f4f842deb3c37920abf0baf54c6f95d5be6d61fd6c20d84c15f47b";

/// The default bridge, from `docker network inspect`, with its attached containers elided.
///
/// Its options are the interesting half: `enable_icc` says whether containers on it can
/// reach each other, `host_binding_ipv4` is the address an unqualified `-p` publishes to,
/// and `name` is the host interface it actually is.
const INSPECT_BRIDGE_NETWORK: &str = r#"[
  {
    "Name": "bridge",
    "Id": "f6267b7bcdf233149bcaccaeb877e708696b17af2347660ce623c8397e4bd1fd",
    "Created": "2026-09-08T12:24:15.457196986Z",
    "Scope": "local",
    "Driver": "bridge",
    "EnableIPv6": false,
    "IPAM": {
      "Driver": "default",
      "Options": null,
      "Config": [{ "Subnet": "172.17.0.0/16", "Gateway": "172.17.0.1" }]
    },
    "Internal": false,
    "Attachable": false,
    "Ingress": false,
    "ConfigFrom": { "Network": "" },
    "ConfigOnly": false,
    "Options": {
      "com.docker.network.bridge.default_bridge": "true",
      "com.docker.network.bridge.enable_icc": "true",
      "com.docker.network.bridge.enable_ip_masquerade": "true",
      "com.docker.network.bridge.host_binding_ipv4": "0.0.0.0",
      "com.docker.network.bridge.name": "docker0",
      "com.docker.network.driver.mtu": "1500"
    },
    "Labels": {},
    "Containers": {}
  }
]"#;

/// A network created with a subnet and nothing else, where docker writes no gateway of its
/// own into the IPAM config and no options at all.
const INSPECT_FIXTURE_NETWORK: &str = r#"[
  {
    "Name": "fixture-net",
    "Id": "1d51e8c10dda55fd904537227940fe186820f04574b31a6a924f628f8f7dcb13",
    "Created": "2026-09-08T11:47:56.095996437Z",
    "Scope": "local",
    "Driver": "bridge",
    "EnableIPv6": false,
    "IPAM": {
      "Driver": "default",
      "Options": {},
      "Config": [{ "Subnet": "172.30.0.0/16" }]
    },
    "Internal": false,
    "Attachable": false,
    "Ingress": false,
    "ConfigFrom": { "Network": "" },
    "ConfigOnly": false,
    "Options": {},
    "Labels": {},
    "Containers": {
      "9087f0af664b107224309443c211e04cda23f03006f76dabea7f72e997d491a3": {
        "Name": "networked",
        "EndpointID": "17b6e00f880af56b51bca33aee9b0a78cdbd9f16c4bbaff974173950e4f04332",
        "MacAddress": "02:42:ac:1e:00:09",
        "IPv4Address": "172.30.0.9/16",
        "IPv6Address": ""
      }
    }
  }
]"#;

/// A volume created with driver options, from `docker volume inspect`.
///
/// The options are the reason this is read at all: a `local` volume with
/// `type=tmpfs`/`device=`/`o=` is not on the disk its mountpoint suggests, and for an NFS
/// volume they name the server the data actually lives on.
const INSPECT_VOLUME_WITH_OPTIONS: &str = r#"[
  {
    "CreatedAt": "2026-09-08T12:28:00Z",
    "Driver": "local",
    "Labels": { "com.example.owner": "platform" },
    "Mountpoint": "/var/lib/docker/volumes/fixture-opts/_data",
    "Name": "fixture-opts",
    "Options": { "device": "tmpfs", "o": "size=32m", "type": "tmpfs" },
    "Scope": "local"
  }
]"#;

/// A volume created with nothing but a name, where docker writes null for both maps.
const INSPECT_PLAIN_VOLUME: &str = r#"[
  {
    "CreatedAt": "2026-09-08T11:45:17Z",
    "Driver": "local",
    "Labels": null,
    "Mountpoint": "/var/lib/docker/volumes/fixture-vol/_data",
    "Name": "fixture-vol",
    "Options": null,
    "Scope": "local"
  }
]"#;

/// A tagged image built on this box, from `docker image inspect`.
///
/// `RepoDigests` is empty because the box built it and never pushed it, and `Parent` is set
/// because the classic builder records a chain where buildkit records none. Both are real
/// states this facet has to carry.
const INSPECT_TAGGED_IMAGE: &str = r#"[
  {
    "Id": "sha256:fa10ef3b6224d65632b644294314ddefb5b9185fb87ba29e9bd8d8c2ba86dc02",
    "RepoTags": ["fixture-app:1", "fixture-app:latest"],
    "RepoDigests": [],
    "Parent": "sha256:d0e93b62e38199f58d210648e86e485c13900f798f372a2cf31032a904c6f232",
    "Comment": "buildkit.dockerfile.v0",
    "Created": "2026-09-08T12:24:37.169201826Z",
    "Size": 8652792,
    "Architecture": "arm64",
    "Variant": "v8",
    "Os": "linux",
    "RootFS": { "Type": "layers", "Layers": ["sha256:b2848c02ac6ff5"] },
    "Metadata": { "LastTagTime": "0001-01-01T00:00:00Z" },
    "Config": {
      "Env": ["PATH=/usr/local/sbin:/usr/local/bin"],
      "Labels": { "org.opencontainers.image.revision": "aaaaaaa" }
    }
  }
]"#;

/// The image the second build displaced, which now has no tags at all.
const INSPECT_DANGLING_IMAGE: &str = r#"[
  {
    "Id": "sha256:f736818d54f4f842deb3c37920abf0baf54c6f95d5be6d61fd6c20d84c15f47b",
    "RepoTags": [],
    "RepoDigests": [],
    "Parent": "sha256:5cd319e9e6b9b915c3cf96dca4bc047438afc07bbd7f8bfe2b9effaf39c1bdb8",
    "Created": "2026-09-08T12:24:34.174181513Z",
    "Size": 8652792,
    "Architecture": "arm64",
    "Variant": "v8",
    "Os": "linux",
    "Config": {
      "Labels": {
        "org.opencontainers.image.revision": "9f8e7d6",
        "org.opencontainers.image.title": "fixture"
      }
    }
  }
]"#;

/// A running container, from `docker inspect --type container`.
///
/// `Config.Hostname` and `HostConfig.NetworkMode` are kept and deliberately unread: the
/// hostname docker generates is the container's own short id, so recording it would put the
/// id in the document twice under a name that suggests it is something else.
const INSPECT_WEB: &str = r#"[
  {
    "Id": "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
    "Created": "2026-09-08T11:00:21.648460605Z",
    "Path": "sh",
    "Args": ["-c", "sleep 3600"],
    "State": {
      "Status": "running",
      "Running": true,
      "Paused": false,
      "Restarting": false,
      "OOMKilled": false,
      "Dead": false,
      "Pid": 358,
      "ExitCode": 0,
      "Error": "",
      "StartedAt": "2026-09-08T11:00:21.680061071Z",
      "FinishedAt": "0001-01-01T00:00:00Z"
    },
    "Image": "sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b",
    "ImageManifestDescriptor": {
      "digest": "sha256:e7a1a92a5bfeee40966aea60f0796b0e7917cc35591542701834f03a68fa3d18"
    },
    "Name": "/web",
    "RestartCount": 0,
    "Driver": "overlayfs",
    "Config": {
      "Image": "alpine",
      "Hostname": "webby",
      "User": "1000:1000",
      "WorkingDir": "/app",
      "Env": [
        "PGPASSWORD=hunter2",
        "PLAIN=visible",
        "DSN=postgres://app:s3cret@db:5432/app?sslmode=require",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
      ],
      "Labels": {
        "com.example.role": "frontend",
        "com.docker.compose.project": "shop",
        "org.opencontainers.image.title": "fixture"
      }
    },
    "NetworkSettings": {
      "Networks": {
        "fixture-net": {
          "IPAMConfig": { "IPv4Address": "172.30.0.9" },
          "Links": null,
          "Aliases": ["shop", "api"],
          "MacAddress": "02:42:ac:1e:00:09",
          "NetworkID": "1d51e8c10dda55fd904537227940fe186820f04574b31a6a924f628f8f7dcb13",
          "EndpointID": "17b6e00f880af56b51bca33aee9b0a78cdbd9f16c4bbaff974173950e4f04332",
          "Gateway": "172.30.0.1",
          "IPAddress": "172.30.0.9",
          "IPPrefixLen": 16,
          "IPv6Gateway": "",
          "GlobalIPv6Address": "",
          "GlobalIPv6PrefixLen": 0,
          "DriverOpts": null,
          "DNSNames": ["networked", "shop", "api", "9087f0af664b"]
        },
        "bridge": {
          "IPAMConfig": null,
          "Links": null,
          "Aliases": null,
          "MacAddress": "02:42:ac:11:00:02",
          "NetworkID": "8b3b9259bbae317187e2614a3462e8471c41cf54acf1b739a0386756457a24e8",
          "EndpointID": "23520eff950676fb3663cde0228300bda92e406f828a325835d3ebf56a042c86",
          "Gateway": "172.17.0.1",
          "IPAddress": "172.17.0.2",
          "IPPrefixLen": 16,
          "IPv6Gateway": "",
          "GlobalIPv6Address": "",
          "GlobalIPv6PrefixLen": 0,
          "DriverOpts": null,
          "DNSNames": null
        }
      },
      "Ports": {
        "7777/tcp": null,
        "80/tcp": [{ "HostIp": "127.0.0.1", "HostPort": "8080" }],
        "9000/udp": [
          { "HostIp": "0.0.0.0", "HostPort": "9000" },
          { "HostIp": "::", "HostPort": "9000" }
        ]
      }
    },
    "Mounts": [
      {
        "Type": "bind",
        "Source": "/etc/hostname",
        "Destination": "/host-name",
        "Mode": "ro",
        "RW": false,
        "Propagation": "rprivate"
      },
      {
        "Type": "volume",
        "Name": "fixture-vol",
        "Source": "/var/lib/docker/volumes/fixture-vol/_data",
        "Destination": "/data",
        "Driver": "local",
        "Mode": "ro",
        "RW": false,
        "Propagation": ""
      }
    ],
    "HostConfig": {
      "AutoRemove": false,
      "NetworkMode": "fixture-net",
      "Tmpfs": { "/scratch": "rw,size=64m" },
      "LogConfig": { "Type": "json-file", "Config": {} },
      "RestartPolicy": { "Name": "unless-stopped", "MaximumRetryCount": 0 },
      "Privileged": false,
      "ReadonlyRootfs": false,
      "CapAdd": null,
      "CapDrop": null,
      "SecurityOpt": null,
      "UsernsMode": "",
      "PidMode": "",
      "IpcMode": "private",
      "CgroupnsMode": "private",
      "Memory": 0,
      "MemorySwap": 0,
      "MemoryReservation": 0,
      "NanoCpus": 0,
      "CpuShares": 0,
      "CpusetCpus": "",
      "PidsLimit": null
    }
  }
]"#;

/// A container that ran and exited non-zero, which is the state a fingerprint is taken to
/// find: `docker ps` alone would not have shown it at all.
const INSPECT_STOPPED: &str = r#"[
  {
    "Id": "551e41b5515383f95ae02559fc0a1cd7500d88be62c2add2d7a2cfdc3746ac5a",
    "Created": "2026-09-08T11:14:12.238184894Z",
    "Path": "sh",
    "Args": ["-c", "exit 3"],
    "State": {
      "Status": "exited",
      "Running": false,
      "Paused": false,
      "Restarting": false,
      "OOMKilled": false,
      "Dead": false,
      "Pid": 0,
      "ExitCode": 3,
      "Error": "",
      "StartedAt": "2026-09-08T11:14:12.261190594Z",
      "FinishedAt": "2026-09-08T11:14:12.30762137Z"
    },
    "Image": "sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b",
    "ImageManifestDescriptor": {
      "digest": "sha256:e7a1a92a5bfeee40966aea60f0796b0e7917cc35591542701834f03a68fa3d18"
    },
    "Name": "/stopped",
    "RestartCount": 0,
    "Driver": "overlayfs",
    "Config": {
      "Image": "alpine",
      "Hostname": "551e41b55153",
      "User": "",
      "WorkingDir": "",
      "Env": ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
      "Labels": {}
    },
    "HostConfig": { "AutoRemove": false, "NetworkMode": "bridge" }
  }
]"#;

/// A container carrying every limit docker can be given, with the values it reported for
/// `--memory 64m --memory-reservation 32m --cpus 1.5 --cpu-shares 512 --pids-limit 100
/// --cpuset-cpus 0-1 --restart on-failure:5`.
const INSPECT_LIMITED: &str = r#"[
  {
    "Id": "3c1f6c1f9f5f4a1d8e2b7c6d5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b",
    "Created": "2026-09-08T11:40:02.000000000Z",
    "Path": "sleep",
    "Args": ["3600"],
    "State": {
      "Status": "running",
      "Running": true,
      "Paused": false,
      "Restarting": false,
      "OOMKilled": false,
      "Dead": false,
      "Pid": 2211,
      "ExitCode": 0,
      "Error": "",
      "StartedAt": "2026-09-08T11:40:02.100000000Z",
      "FinishedAt": "0001-01-01T00:00:00Z",
      "Health": {
        "Status": "unhealthy",
        "FailingStreak": 2,
        "Log": [
          {
            "Start": "2026-09-08T11:41:02.000000000Z",
            "End": "2026-09-08T11:41:02.100000000Z",
            "ExitCode": 1,
            "Output": "psql: FATAL: password authentication failed for user \"app\""
          }
        ]
      }
    },
    "Image": "sha256:1991bd789d7184290c3cce84fd6af068b8b745e9bddf178661ce7f5ecf68135c",
    "Name": "/limited",
    "RestartCount": 0,
    "Driver": "overlayfs",
    "Config": {
      "Image": "alpine",
      "Hostname": "3c1f6c1f9f5f",
      "User": "",
      "WorkingDir": "",
      "Env": ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
      "Labels": {},
      "Healthcheck": {
        "Test": ["CMD-SHELL", "true"],
        "Interval": 30000000000,
        "Timeout": 5000000000,
        "StartPeriod": 10000000000,
        "Retries": 3
      }
    },
    "HostConfig": {
      "AutoRemove": false,
      "NetworkMode": "bridge",
      "LogConfig": { "Type": "json-file", "Config": { "max-file": "3", "max-size": "1m" } },
      "RestartPolicy": { "Name": "on-failure", "MaximumRetryCount": 5 },
      "Privileged": false,
      "ReadonlyRootfs": true,
      "CapAdd": ["NET_ADMIN", "SYS_TIME"],
      "CapDrop": ["CHOWN"],
      "SecurityOpt": ["no-new-privileges", "label=disable"],
      "UsernsMode": "host",
      "PidMode": "host",
      "IpcMode": "none",
      "CgroupnsMode": "private",
      "Memory": 67108864,
      "MemorySwap": 134217728,
      "MemoryReservation": 33554432,
      "NanoCpus": 1500000000,
      "CpuShares": 512,
      "CpusetCpus": "0-1",
      "PidsLimit": 100
    }
  }
]"#;

/// A container started with `--rm`, which will delete itself the moment it stops.
const INSPECT_EPHEMERAL: &str = r#"[
  {
    "Id": "1db8c55268930421b2a804afd47b84b7ff88c7ee942242c529431fef314c5ea6",
    "Created": "2026-09-08T11:14:12.054894501Z",
    "Path": "sleep",
    "Args": ["3600"],
    "State": {
      "Status": "running",
      "Running": true,
      "Paused": false,
      "Restarting": false,
      "OOMKilled": false,
      "Dead": false,
      "Pid": 1309,
      "ExitCode": 0,
      "Error": "",
      "StartedAt": "2026-09-08T11:14:12.080057163Z",
      "FinishedAt": "0001-01-01T00:00:00Z"
    },
    "Image": "sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b",
    "ImageManifestDescriptor": {
      "digest": "sha256:e7a1a92a5bfeee40966aea60f0796b0e7917cc35591542701834f03a68fa3d18"
    },
    "Name": "/ephemeral",
    "RestartCount": 0,
    "Driver": "overlayfs",
    "Config": {
      "Image": "alpine",
      "Hostname": "1db8c5526893",
      "User": "",
      "WorkingDir": "",
      "Env": ["EMPTY=", "EQUALS=a=b=c"],
      "Labels": {}
    },
    "HostConfig": { "AutoRemove": true, "NetworkMode": "bridge" }
  }
]"#;

/// What one fake docker answers with.
///
/// A named type rather than five positional arguments: every one of these is a fixture a
/// test chose, and at a call site `containers: &[]` says something a bare `&[]` would not.
struct DockerFixtures<'a> {
    version: &'a str,
    /// What the client writes to stderr while exiting zero, which is where an unreachable
    /// daemon reports itself.
    version_stderr: &'a str,
    info: &'a str,
    /// The containers `docker ps` lists, each with the `inspect` document for it. An id
    /// listed with no document is how a test drives the container that vanished mid-read.
    containers: &'a [(&'a str, Option<&'a str>)],
    /// The images `docker image ls` lists, on the same terms.
    images: &'a [(&'a str, Option<&'a str>)],
    /// The volumes `docker volume ls` lists, on the same terms.
    volumes: &'a [(&'a str, Option<&'a str>)],
    /// The networks `docker network ls` lists, on the same terms.
    networks: &'a [(&'a str, Option<&'a str>)],
}

impl DockerFixtures<'_> {
    /// A box whose daemon answers, running the three fixture containers.
    fn answering() -> Self {
        Self {
            version: VERSION_ANSWERING,
            version_stderr: "",
            info: INFO_ANSWERING,
            containers: &[
                (WEB_ID, Some(INSPECT_WEB)),
                (STOPPED_ID, Some(INSPECT_STOPPED)),
                (EPHEMERAL_ID, Some(INSPECT_EPHEMERAL)),
                (LIMITED_ID, Some(INSPECT_LIMITED)),
            ],
            images: &[
                (TAGGED_IMAGE, Some(INSPECT_TAGGED_IMAGE)),
                (DANGLING_IMAGE, Some(INSPECT_DANGLING_IMAGE)),
            ],
            volumes: &[
                ("fixture-opts", Some(INSPECT_VOLUME_WITH_OPTIONS)),
                ("fixture-vol", Some(INSPECT_PLAIN_VOLUME)),
            ],
            networks: &[
                (BRIDGE_NETWORK_ID, Some(INSPECT_BRIDGE_NETWORK)),
                (FIXTURE_NETWORK_ID, Some(INSPECT_FIXTURE_NETWORK)),
            ],
        }
    }
}

/// A `docker` that answers from fixtures, and refuses anything else loudly.
///
/// Refusing the unexpected is what makes the shim a test rather than a mock that agrees with
/// whatever it is asked: a source that started calling a subcommand nobody wrote a fixture
/// for fails here instead of quietly reading an empty answer.
fn fake_docker(name: &str, fixtures: DockerFixtures) -> Docker {
    let root = scratch_tree(&format!("containers-{name}"), &[]);

    let mut ids = String::new();
    for (id, document) in fixtures.containers {
        ids.push_str(id);
        ids.push('\n');
        if let Some(document) = document {
            fs::write(root.join(format!("{id}.json")), document).expect("a writable fixture");
        }
    }
    fs::write(root.join("ids"), &ids).expect("a writable fixture");

    let mut image_ids = String::new();
    for (id, document) in fixtures.images {
        image_ids.push_str(id);
        image_ids.push('\n');
        if let Some(document) = document {
            fs::write(root.join(format!("{id}.json")), document).expect("a writable fixture");
        }
    }
    fs::write(root.join("image-ids"), &image_ids).expect("a writable fixture");

    let mut volume_names = String::new();
    for (name, document) in fixtures.volumes {
        volume_names.push_str(name);
        volume_names.push('\n');
        if let Some(document) = document {
            fs::write(root.join(format!("volume-{name}.json")), document)
                .expect("a writable fixture");
        }
    }
    fs::write(root.join("volume-names"), &volume_names).expect("a writable fixture");

    let mut network_ids = String::new();
    for (id, document) in fixtures.networks {
        network_ids.push_str(id);
        network_ids.push('\n');
        if let Some(document) = document {
            fs::write(root.join(format!("network-{id}.json")), document)
                .expect("a writable fixture");
        }
    }
    fs::write(root.join("network-ids"), &network_ids).expect("a writable fixture");

    let directory = root.to_str().expect("a UTF-8 scratch path");
    let path = root.join("docker");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
case "$1" in
version)
cat <<'STDOUT'
{version}
STDOUT
printf '%s' '{version_stderr}' >&2
;;
info)
cat <<'STDOUT'
{info}
STDOUT
;;
ps)
cat '{directory}/ids'
;;
network)
case "$2" in
ls)
cat '{directory}/network-ids'
;;
inspect)
document='{directory}/network-'"$3"'.json'
if [ -f "$document" ]; then
cat "$document"
else
printf 'Error response from daemon: network %s not found\n' "$3" >&2
exit 1
fi
;;
*)
printf 'unexpected network invocation: %s\n' "$*" >&2
exit 1
;;
esac
;;
volume)
case "$2" in
ls)
cat '{directory}/volume-names'
;;
inspect)
document='{directory}/volume-'"$3"'.json'
if [ -f "$document" ]; then
cat "$document"
else
printf 'Error response from daemon: get %s: no such volume\n' "$3" >&2
exit 1
fi
;;
*)
printf 'unexpected volume invocation: %s\n' "$*" >&2
exit 1
;;
esac
;;
image)
case "$2" in
ls)
cat '{directory}/image-ids'
;;
inspect)
document='{directory}/'"$3"'.json'
if [ -f "$document" ]; then
cat "$document"
else
printf 'Error response from daemon: No such image: %s\n' "$3" >&2
exit 1
fi
;;
*)
printf 'unexpected image invocation: %s\n' "$*" >&2
exit 1
;;
esac
;;
inspect)
document='{directory}/'"$4"'.json'
if [ -f "$document" ]; then
cat "$document"
else
printf 'Error response from daemon: No such container: %s\n' "$4" >&2
exit 1
fi
;;
*)
printf 'unexpected invocation: %s\n' "$*" >&2
exit 1
;;
esac
"#,
            version = fixtures.version,
            version_stderr = fixtures.version_stderr,
            info = fixtures.info,
            directory = directory,
        ),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");

    Docker::using(
        CanonicalTool::located_in("docker", &[directory]).expect("the fake tool is locatable"),
    )
}

fn docker_facet(name: &str, fixtures: DockerFixtures) -> Observation {
    ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(name, fixtures))])
        .collect()
        .expect("the fixtures are well formed")
}

fn answering_server(name: &str) -> Observation {
    field(
        &field(&docker_facet(name, DockerFixtures::answering()), "docker"),
        "server",
    )
}

fn container_of(name: &str, container: &str) -> Observation {
    field(&field(&answering_server(name), "containers"), container)
}

#[test]
fn presence_is_absent_when_no_container_engine_is_on_the_host() {
    // Arrange
    let collector = ContainersCollector::reading(Vec::new());

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_present_when_docker_is_on_the_host() {
    // Arrange
    let collector = ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(
        "present",
        DockerFixtures::answering(),
    ))]);

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_facet_is_keyed_by_the_engine() {
    // Act
    let observed = docker_facet("keyed", DockerFixtures::answering());

    // Assert
    assert_eq!(keys_of(&observed), vec!["docker".to_owned()]);
}

#[test]
fn an_answering_docker_reports_what_its_daemon_is_running_with() {
    // Act
    let observed = docker_facet("answering", DockerFixtures::answering());
    let docker = field(&observed, "docker");
    let server = field(&docker, "server");

    // Assert
    assert_eq!(text(&field(&docker, "client_version")), "29.8.0");
    assert_eq!(text(&field(&docker, "daemon")), "answering");
    assert_eq!(text(&field(&server, "version")), "29.8.0");
    assert_eq!(text(&field(&server, "root_directory")), "/var/lib/docker");
    assert_eq!(text(&field(&server, "storage_driver")), "overlayfs");
    assert_eq!(text(&field(&server, "logging_driver")), "json-file");
    assert_eq!(text(&field(&server, "default_runtime")), "runc");
    assert_eq!(text(&field(&server, "swarm")), "inactive");
    assert!(!boolean(&field(&server, "live_restore")));
}

#[test]
fn a_cgroup_driver_and_version_are_reported_as_the_daemon_resolved_them() {
    // Arrange: the pair decides whether a container's memory and pids limits can apply at
    // all, so a v1 box and a v2 box are different state even with identical containers.
    let cgroup = field(&answering_server("cgroup"), "cgroup");

    // Act & Assert
    assert_eq!(text(&field(&cgroup, "driver")), "cgroupfs");
    assert_eq!(text(&field(&cgroup, "version")), "2");
}

#[test]
fn the_security_options_are_sorted_rather_than_left_in_the_daemons_order() {
    // Arrange: docker promises nothing about the order of this list, and an order that moved
    // between two runs of an unchanged box would break byte-identity.
    let options = items_of(&field(&answering_server("security"), "security_options"));

    // Act & Assert
    assert_eq!(
        options.iter().map(text).collect::<Vec<String>>(),
        vec![
            "name=cgroupns".to_owned(),
            "name=seccomp,profile=builtin".to_owned()
        ]
    );
}

#[test]
fn the_components_report_which_containerd_and_runc_the_engine_runs() {
    // Arrange
    let components = field(&answering_server("components"), "components");

    // Act & Assert
    assert_eq!(text(&field(&components, "containerd")), "v2.3.4");
    assert_eq!(text(&field(&components, "runc")), "1.5.1");
}

#[test]
fn a_docker_whose_daemon_does_not_answer_is_installed_and_unreachable() {
    // Arrange: the box has docker and no running daemon, which is state rather than a
    // failure to read, and is a different fact from having no docker at all.
    let observed = docker_facet(
        "unreachable",
        DockerFixtures {
            version: VERSION_UNREACHABLE,
            version_stderr: UNREACHABLE_STDERR,
            info: "",
            containers: &[],
            images: &[],
            volumes: &[],
            networks: &[],
        },
    );

    // Act
    let docker = field(&observed, "docker");

    // Assert
    assert_eq!(text(&field(&docker, "client_version")), "29.8.0");
    assert_eq!(text(&field(&docker, "daemon")), "unreachable");
    assert!(text(&field(&docker, "daemon_reason")).contains("docker.sock"));
    assert!(is_null(&field(&docker, "server")));
}

#[test]
fn an_answering_daemon_records_no_reason_to_be_unreachable() {
    // Arrange
    let docker = field(
        &docker_facet("no-reason", DockerFixtures::answering()),
        "docker",
    );

    // Act & Assert
    assert!(is_null(&field(&docker, "daemon_reason")));
}

#[test]
fn output_that_is_not_json_fails_the_facet_rather_than_reading_as_an_empty_engine() {
    // Arrange
    let collector = ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(
        "garbage",
        DockerFixtures {
            version: "not json at all",
            version_stderr: "",
            info: INFO_ANSWERING,
            containers: &[],
            images: &[],
            volumes: &[],
            networks: &[],
        },
    ))]);

    // Act
    let failure = collector.collect().expect_err("garbage is not an engine");

    // Assert
    assert!(
        failure.to_string().contains("docker version"),
        "the failure should name the probe that produced it: {failure}"
    );
}

#[test]
fn the_containers_are_keyed_by_name_without_dockers_leading_slash() {
    // Arrange: docker reports a container's name as `/web`, a leftover from the days when
    // links made a namespace of it. The name an operator uses is what the document keys on.
    let containers = field(&answering_server("names"), "containers");

    // Act & Assert
    assert_eq!(
        keys_of(&containers),
        vec![
            "ephemeral".to_owned(),
            "limited".to_owned(),
            "stopped".to_owned(),
            "web".to_owned()
        ]
    );
}

#[test]
fn a_container_records_the_image_it_asked_for_beside_the_one_it_got() {
    // Arrange: the whole reason this facet exists. `alpine` is what the operator wrote and
    // what a config file would show; the digest is what is actually running, and a tag
    // repointed at a new build changes the second while the first stands still.
    let image = field(&container_of("image", "web"), "image");

    // Act & Assert
    assert_eq!(text(&field(&image, "reference")), "alpine");
    assert_eq!(
        text(&field(&image, "id")),
        "sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b"
    );
    assert_eq!(
        text(&field(&image, "manifest_digest")),
        "sha256:e7a1a92a5bfeee40966aea60f0796b0e7917cc35591542701834f03a68fa3d18"
    );
}

#[test]
fn a_container_records_the_command_the_engine_resolved_rather_than_the_one_configured() {
    // Arrange: `Path` and `Args` are what the container actually runs, after docker has
    // resolved the image's entrypoint against the command it was given.
    let command = field(&container_of("command", "web"), "command");

    // Act & Assert
    assert_eq!(text(&field(&command, "path")), "sh");
    assert_eq!(
        items_of(&field(&command, "arguments"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["-c".to_owned(), "sleep 3600".to_owned()]
    );
}

#[test]
fn a_running_container_records_no_finish_stamp() {
    // Arrange: docker fills the field with Go's zero time, `0001-01-01T00:00:00Z`, which is
    // not a date the container finished at and must not read as one.
    let state = field(&container_of("running", "web"), "state");

    // Act & Assert
    assert_eq!(text(&field(&state, "status")), "running");
    assert!(is_null(&field(&state, "finished_at")));
}

#[test]
fn an_exited_container_records_its_exit_code_and_when_it_finished() {
    // Arrange
    let state = field(&container_of("exited", "stopped"), "state");

    // Act & Assert
    assert_eq!(text(&field(&state, "status")), "exited");
    assert_eq!(integer(&field(&state, "exit_code")), 3);
    assert_eq!(
        text(&field(&state, "finished_at")),
        "2026-09-08T11:14:12.30762137Z"
    );
}

#[test]
fn the_stamps_and_the_restart_count_are_volatile_while_the_status_is_not() {
    // Arrange: a container restarting on its own moves both stamps and the count without
    // anybody changing the box, and byte-identity is what every other facet rests on. The
    // status is the opposite: `running` becoming `exited` is the change worth diffing.
    let state = field(&container_of("volatility", "web"), "state");

    // Act & Assert
    assert_eq!(
        field(&state, "started_at").volatility(),
        Volatility::Volatile
    );
    assert_eq!(
        field(&state, "restart_count").volatility(),
        Volatility::Volatile
    );
    assert_eq!(field(&state, "status").volatility(), Volatility::Stable);
}

#[test]
fn a_container_that_will_delete_itself_is_volatile_whole() {
    // Arrange: `--rm` says the container is a job rather than a tenant, and a cron-driven
    // one appears and vanishes between two runs of a box nobody touched. Keyed on
    // `AutoRemove` rather than guessed from a name, because that is the engine's own record
    // of the intent.
    let ephemeral = container_of("ephemeral", "ephemeral");

    // Act & Assert
    assert_eq!(ephemeral.volatility(), Volatility::Volatile);
    assert_eq!(
        container_of("ephemeral", "web").volatility(),
        Volatility::Stable
    );
}

#[test]
fn a_container_that_vanished_while_being_read_is_recorded_rather_than_dropped() {
    // Arrange: a `docker run --rm` from cron can end between the id list and the inspect of
    // it. Recording the loss keeps the omission visible, and the entry is volatile because a
    // container that comes and goes on its own is the host changing on its own.
    let server = field(
        &field(
            &docker_facet(
                "vanished",
                DockerFixtures {
                    version: VERSION_ANSWERING,
                    version_stderr: "",
                    info: INFO_ANSWERING,
                    containers: &[(WEB_ID, Some(INSPECT_WEB)), (EPHEMERAL_ID, None)],
                    images: &[],
                    volumes: &[],
                    networks: &[],
                },
            ),
            "docker",
        ),
        "server",
    );
    let unreadable = field(&server, "unreadable_containers");

    // Act & Assert
    assert_eq!(
        keys_of(&field(&server, "containers")),
        vec!["web".to_owned()]
    );
    assert_eq!(unreadable.volatility(), Volatility::Volatile);
    let entries = items_of(&unreadable);
    assert_eq!(entries.len(), 1);
    assert_eq!(text(&field(&entries[0], "id")), EPHEMERAL_ID);
    assert!(text(&field(&entries[0], "reason")).contains("No such container"));
}

#[test]
fn a_daemon_with_no_containers_reports_an_empty_list_rather_than_nothing() {
    // Arrange: an engine installed and running with nothing on it is a real state, and a
    // different one from an engine that could not be asked.
    let server = field(
        &field(
            &docker_facet(
                "empty",
                DockerFixtures {
                    version: VERSION_ANSWERING,
                    version_stderr: "",
                    info: INFO_ANSWERING,
                    containers: &[],
                    images: &[],
                    volumes: &[],
                    networks: &[],
                },
            ),
            "docker",
        ),
        "server",
    );

    // Act & Assert
    assert!(keys_of(&field(&server, "containers")).is_empty());
    assert!(items_of(&field(&server, "unreadable_containers")).is_empty());
}

#[test]
fn the_environment_is_keyed_by_variable_and_every_value_is_sensitive() {
    // Arrange: not a judgement about which variables hold secrets, because the name cannot
    // tell. `DSN=postgres://app:s3cret@db/app` carries a credential and matches no keyword a
    // rule could look for, and a rule that guesses fails in the direction that leaks.
    let environment = field(&container_of("environment", "web"), "environment");

    // Act & Assert
    assert_eq!(
        keys_of(&environment),
        vec![
            "DSN".to_owned(),
            "PATH".to_owned(),
            "PGPASSWORD".to_owned(),
            "PLAIN".to_owned()
        ]
    );
    for variable in keys_of(&environment) {
        assert_eq!(
            field(&environment, &variable).sensitivity(),
            Sensitivity::Sensitive,
            "{variable} should be sensitive whatever it is called"
        );
    }
}

#[test]
fn an_environment_value_holding_an_equals_sign_keeps_all_of_it() {
    // Arrange: the entry is `NAME=value` and the value may hold as many `=` as it likes, so
    // the split is on the first one only. Splitting on every one would corrupt a DSN.
    let environment = field(&container_of("equals", "ephemeral"), "environment");

    // Act & Assert
    assert_eq!(text(&field(&environment, "EQUALS")), "a=b=c");
}

#[test]
fn a_variable_set_to_nothing_is_recorded_as_empty_rather_than_dropped() {
    // Arrange: `--env EMPTY=` is a variable the container has, set to nothing, and that is a
    // different fact from the variable not being there at all.
    let environment = field(&container_of("empty-variable", "ephemeral"), "environment");

    // Act & Assert
    assert_eq!(text(&field(&environment, "EMPTY")), "");
}

#[test]
fn the_labels_are_recorded_as_the_engine_holds_them() {
    // Arrange: compose writes its project and service into labels, which makes them the
    // anchor a diff needs for a container it did not name itself.
    let labels = field(&container_of("labels", "web"), "labels");

    // Act & Assert
    assert_eq!(text(&field(&labels, "com.docker.compose.project")), "shop");
    assert_eq!(text(&field(&labels, "com.example.role")), "frontend");
}

#[test]
fn a_container_records_the_account_and_the_directory_it_runs_in() {
    // Arrange: recorded as docker spells it. A `uid:gid` pair is not resolved to names,
    // because the passwd file that would resolve it is the container's own and this
    // collector does not open files inside a container.
    let container = container_of("account", "web");

    // Act & Assert
    assert_eq!(text(&field(&container, "user")), "1000:1000");
    assert_eq!(text(&field(&container, "working_directory")), "/app");
}

#[test]
fn an_account_or_directory_the_image_decides_is_absent_rather_than_empty() {
    // Arrange: docker reports an empty string when the container overrode neither, and
    // recording that as text would claim the container runs as a nameless account in a
    // directory with no path.
    let container = container_of("image-default", "stopped");

    // Act & Assert
    assert!(is_null(&field(&container, "user")));
    assert!(is_null(&field(&container, "working_directory")));
}

#[test]
fn the_mounts_are_keyed_by_the_path_inside_the_container() {
    // Arrange: docker returned these two in the opposite order from the one they were
    // declared in, measured on 26.1.5, so its order is docker's own and not something to
    // put in a document that has to be byte-identical. A destination is unique per
    // container, which makes it the key.
    let mounts = field(&container_of("mounts", "web"), "mounts");

    // Act & Assert
    assert_eq!(
        keys_of(&mounts),
        vec![
            "/data".to_owned(),
            "/host-name".to_owned(),
            "/scratch".to_owned()
        ]
    );
}

#[test]
fn a_volume_mount_records_the_volume_and_where_the_engine_keeps_it() {
    // Arrange
    let mount = field(
        &field(&container_of("volume-mount", "web"), "mounts"),
        "/data",
    );

    // Act & Assert
    assert_eq!(text(&field(&mount, "kind")), "volume");
    assert_eq!(text(&field(&mount, "name")), "fixture-vol");
    assert_eq!(
        text(&field(&mount, "source")),
        "/var/lib/docker/volumes/fixture-vol/_data"
    );
    assert_eq!(text(&field(&mount, "driver")), "local");
    assert!(!boolean(&field(&mount, "writable")));
}

#[test]
fn a_bind_mount_records_the_host_path_and_its_propagation() {
    // Arrange: a bind is the mount that reaches out of the container, so the host path is
    // the value that matters, and the propagation says whether a mount made on the host
    // afterwards appears inside it.
    let mount = field(
        &field(&container_of("bind-mount", "web"), "mounts"),
        "/host-name",
    );

    // Act & Assert
    assert_eq!(text(&field(&mount, "kind")), "bind");
    assert_eq!(text(&field(&mount, "source")), "/etc/hostname");
    assert_eq!(text(&field(&mount, "propagation")), "rprivate");
    assert!(is_null(&field(&mount, "name")));
}

#[test]
fn a_tmpfs_mount_is_read_from_the_only_place_docker_reports_it() {
    // Arrange: **measured, and it decides the shape of this read.** A `--tmpfs` mount does
    // not appear in `Mounts` at all. It is only in `HostConfig.Tmpfs`, as a destination
    // mapped to its options, so a facet reading `Mounts` alone would silently lose every
    // tmpfs on the box.
    let mount = field(&field(&container_of("tmpfs", "web"), "mounts"), "/scratch");

    // Act & Assert
    assert_eq!(text(&field(&mount, "kind")), "tmpfs");
    assert_eq!(text(&field(&mount, "options")), "rw,size=64m");
    assert!(is_null(&field(&mount, "source")));
    assert!(boolean(&field(&mount, "writable")));
}

#[test]
fn the_ports_are_keyed_the_way_the_engine_names_them() {
    // Arrange: `80/tcp` is the engine's own name for a port, and the one an operator reads
    // out of `docker ps`, so it is what the document keys on.
    let ports = field(&container_of("ports", "web"), "ports");

    // Act & Assert
    assert_eq!(
        keys_of(&ports),
        vec![
            "7777/tcp".to_owned(),
            "80/tcp".to_owned(),
            "9000/udp".to_owned()
        ]
    );
}

#[test]
fn a_port_exposed_and_published_nowhere_is_recorded_with_no_bindings() {
    // Arrange: docker reports `"7777/tcp": null` for a port the image exposes and nobody
    // published. That is real state, and a different fact from the port not being there:
    // the container listens on it, and only the box can reach it.
    let ports = field(&container_of("exposed", "web"), "ports");

    // Act & Assert
    assert!(items_of(&field(&ports, "7777/tcp")).is_empty());
}

#[test]
fn a_published_port_records_where_it_is_reachable_from() {
    // Arrange: the whole point of reading this. `127.0.0.1:8080` is reachable from the box
    // and `0.0.0.0:9000` is reachable from the network, and which of the two a port is
    // published on is the difference a fingerprint is taken to catch.
    let ports = field(&container_of("published", "web"), "ports");
    let loopback = items_of(&field(&ports, "80/tcp"));
    let wildcards = items_of(&field(&ports, "9000/udp"));

    // Act & Assert
    assert_eq!(text(&field(&loopback[0], "host_address")), "127.0.0.1");
    assert_eq!(integer(&field(&loopback[0], "host_port")), 8080);
    assert_eq!(
        wildcards
            .iter()
            .map(|binding| text(&field(binding, "host_address")))
            .collect::<Vec<String>>(),
        vec!["0.0.0.0".to_owned(), "::".to_owned()]
    );
}

#[test]
fn the_bindings_of_one_port_are_sorted_rather_than_left_in_the_engines_order() {
    // Arrange: publishing one port without naming an address gives two bindings, one per
    // family, and docker promises nothing about which comes first.
    let bindings = items_of(&field(
        &field(&container_of("binding-order", "web"), "ports"),
        "9000/udp",
    ));

    // Act & Assert
    let addresses: Vec<String> = bindings
        .iter()
        .map(|binding| text(&field(binding, "host_address")))
        .collect();
    let mut sorted = addresses.clone();
    sorted.sort();
    assert_eq!(addresses, sorted);
}

#[test]
fn the_networks_are_keyed_by_name_with_their_aliases_sorted() {
    // Arrange: a container on two networks is on two networks, and the name is what both
    // the operator and the other containers know it by. The aliases arrive in the order
    // they were declared, which is the operator's order and not something to depend on.
    let networks = field(&container_of("networks", "web"), "networks");

    // Act & Assert
    assert_eq!(
        keys_of(&networks),
        vec!["bridge".to_owned(), "fixture-net".to_owned()]
    );
    assert_eq!(
        items_of(&field(&field(&networks, "fixture-net"), "aliases"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["api".to_owned(), "shop".to_owned()]
    );
}

#[test]
fn a_networks_entry_records_the_address_and_the_network_it_is_on() {
    // Arrange: the network id is kept because a network destroyed and recreated under the
    // same name is a different network, and the id is the only witness to that.
    let network = field(
        &field(&container_of("addressed", "web"), "networks"),
        "fixture-net",
    );

    // Act & Assert
    assert_eq!(text(&field(&network, "address")), "172.30.0.9");
    assert_eq!(
        text(&field(&network, "hardware_address")),
        "02:42:ac:1e:00:09"
    );
    assert_eq!(
        text(&field(&network, "network_id")),
        "1d51e8c10dda55fd904537227940fe186820f04574b31a6a924f628f8f7dcb13"
    );
}

#[test]
fn a_static_address_that_was_asked_for_is_recorded_beside_the_one_assigned() {
    // Arrange: the pair is the point. A compose file naming a fixed address is a
    // declaration, and an engine that assigned something else is exactly the disagreement
    // a fingerprint is taken to surface.
    let network = field(
        &field(&container_of("static", "web"), "networks"),
        "fixture-net",
    );

    // Act & Assert
    assert_eq!(text(&field(&network, "requested_address")), "172.30.0.9");
}

#[test]
fn a_network_that_was_asked_for_nothing_records_no_request() {
    // Arrange: on the default bridge docker leaves `IPAMConfig` and `Aliases` null, and an
    // address nobody asked for must not read as one that was requested and honoured.
    let bridge = field(&field(&container_of("bridge", "web"), "networks"), "bridge");

    // Act & Assert
    assert_eq!(text(&field(&bridge, "address")), "172.17.0.2");
    assert!(is_null(&field(&bridge, "requested_address")));
    assert!(items_of(&field(&bridge, "aliases")).is_empty());
}

#[test]
fn a_container_with_no_ipv6_address_records_none_rather_than_empty_text() {
    // Arrange: docker writes `"GlobalIPv6Address": ""` on a network with no IPv6 at all,
    // and empty text would claim an address that is nothing.
    let bridge = field(
        &field(&container_of("no-ipv6", "web"), "networks"),
        "bridge",
    );

    // Act & Assert
    assert!(is_null(&field(&bridge, "ipv6_address")));
}

#[test]
fn a_restart_policy_with_a_retry_limit_records_both_halves() {
    // Arrange: whether a crashed container comes back, and how many times, is the
    // difference between a service that heals and one that flaps.
    let policy = field(&container_of("policy", "limited"), "restart_policy");

    // Act & Assert
    assert_eq!(text(&field(&policy, "name")), "on-failure");
    assert_eq!(integer(&field(&policy, "maximum_retries")), 5);
}

#[test]
fn a_restart_policy_that_names_no_retry_limit_records_none() {
    // Arrange: docker writes `MaximumRetryCount: 0` for every policy that does not use it,
    // and recording that as zero would read as "never retry", which is the opposite of what
    // `unless-stopped` does.
    let policy = field(&container_of("no-retries", "web"), "restart_policy");

    // Act & Assert
    assert_eq!(text(&field(&policy, "name")), "unless-stopped");
    assert!(is_null(&field(&policy, "maximum_retries")));
}

#[test]
fn the_limits_are_recorded_in_the_units_the_engine_reports_them_in() {
    // Arrange: bytes and nanocpus, both integers, which is why `--cpus 1.5` can be recorded
    // at all: the document admits no floating point, and docker's own unit for a fractional
    // CPU is a whole number of billionths.
    let limits = field(&container_of("limits", "limited"), "limits");

    // Act & Assert
    assert_eq!(integer(&field(&limits, "memory_bytes")), 67_108_864);
    assert_eq!(integer(&field(&limits, "memory_swap_bytes")), 134_217_728);
    assert_eq!(
        integer(&field(&limits, "memory_reservation_bytes")),
        33_554_432
    );
    assert_eq!(integer(&field(&limits, "nano_cpus")), 1_500_000_000);
    assert_eq!(integer(&field(&limits, "cpu_shares")), 512);
    assert_eq!(integer(&field(&limits, "process_limit")), 100);
    assert_eq!(text(&field(&limits, "cpu_set")), "0-1");
}

#[test]
fn a_limit_the_container_does_not_have_is_absent_rather_than_zero() {
    // Arrange: docker writes 0 for an unset memory or cpu limit, null for an unset pids
    // limit and an empty string for an unset cpu set. A memory limit recorded as 0 would
    // read as a container confined to no memory at all, which is the opposite of the truth.
    let limits = field(&container_of("unlimited", "web"), "limits");

    // Act & Assert
    assert!(is_null(&field(&limits, "memory_bytes")));
    assert!(is_null(&field(&limits, "nano_cpus")));
    assert!(is_null(&field(&limits, "cpu_shares")));
    assert!(is_null(&field(&limits, "process_limit")));
    assert!(is_null(&field(&limits, "cpu_set")));
}

#[test]
fn a_hardened_container_records_what_it_added_dropped_and_forbade() {
    // Arrange: sorted, because the engine keeps them in the order they were given and an
    // operator reordering two `--cap-add` flags has not changed the box.
    //
    // `label=disable` was not asked for: docker added it because `--pid host` makes SELinux
    // labelling impossible, which is why the effective list is the one worth recording.
    let security = field(&container_of("hardened", "limited"), "security");
    let capabilities = field(&security, "capabilities");

    // Act & Assert
    assert!(boolean(&field(&security, "read_only_root_filesystem")));
    assert_eq!(
        items_of(&field(&capabilities, "added"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["NET_ADMIN".to_owned(), "SYS_TIME".to_owned()]
    );
    assert_eq!(
        items_of(&field(&capabilities, "dropped"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["CHOWN".to_owned()]
    );
    assert_eq!(
        items_of(&field(&security, "options"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["label=disable".to_owned(), "no-new-privileges".to_owned()]
    );
}

#[test]
fn the_namespaces_record_which_of_them_the_container_shares_with_the_host() {
    // Arrange: `--pid host` is the flag that makes a container able to see and signal every
    // process on the box, and `--userns host` is the one that makes root inside it root
    // outside it. Both are one word in a document nobody would otherwise diff.
    let namespaces = field(
        &field(&container_of("namespaces", "limited"), "security"),
        "namespaces",
    );

    // Act & Assert
    assert_eq!(text(&field(&namespaces, "process")), "host");
    assert_eq!(text(&field(&namespaces, "user")), "host");
    assert_eq!(text(&field(&namespaces, "interprocess")), "none");
    assert_eq!(text(&field(&namespaces, "control_group")), "private");
}

#[test]
fn a_container_that_changed_nothing_records_no_capability_changes() {
    // Arrange: docker writes null for the lists and an empty string for a mode the
    // container did not choose, and neither is a change the container made.
    let security = field(&container_of("plain", "web"), "security");
    let namespaces = field(&security, "namespaces");

    // Act & Assert
    assert!(!boolean(&field(&security, "privileged")));
    assert!(!boolean(&field(&security, "read_only_root_filesystem")));
    assert!(items_of(&field(&field(&security, "capabilities"), "added")).is_empty());
    assert!(items_of(&field(&security, "options")).is_empty());
    assert!(is_null(&field(&namespaces, "process")));
    assert!(is_null(&field(&namespaces, "user")));
}

#[test]
fn the_network_namespace_records_whichever_of_the_four_things_it_can_be() {
    // Arrange: `NetworkMode` is a network's name on one container and a namespace choice on
    // another, `host` being the one that puts the container on the box's own stack. It is
    // recorded as the engine spells it rather than sorted into two fields, because the
    // engine keeps one field and the reader needs to see which of the two it holds.
    let plain = field(&container_of("network-mode", "web"), "security");

    // Act & Assert
    assert_eq!(
        text(&field(&field(&plain, "namespaces"), "network")),
        "fixture-net"
    );
}

#[test]
fn a_healthcheck_records_its_command_and_its_timings() {
    // Arrange: nanoseconds, because that is docker's own unit and the document admits no
    // floating point, so `--health-interval 30s` is carried exactly.
    let healthcheck = field(&container_of("healthcheck", "limited"), "healthcheck");

    // Act & Assert
    assert_eq!(
        items_of(&field(&healthcheck, "test"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["CMD-SHELL".to_owned(), "true".to_owned()]
    );
    assert_eq!(
        integer(&field(&healthcheck, "interval_nanoseconds")),
        30_000_000_000
    );
    assert_eq!(
        integer(&field(&healthcheck, "timeout_nanoseconds")),
        5_000_000_000
    );
    assert_eq!(
        integer(&field(&healthcheck, "start_period_nanoseconds")),
        10_000_000_000
    );
    assert_eq!(integer(&field(&healthcheck, "retries")), 3);
}

#[test]
fn a_container_with_no_healthcheck_records_none() {
    // Arrange: docker writes null for a container whose image declares none and which asked
    // for none, and an empty healthcheck object would claim one that never runs.
    let container = container_of("no-healthcheck", "web");

    // Act & Assert
    assert!(is_null(&field(&container, "healthcheck")));
}

#[test]
fn the_configured_healthcheck_is_stable_while_the_health_it_observes_is_volatile() {
    // Arrange: the check is configuration and does not move; whether it is currently passing
    // moves on its own, which is the definition of volatile, and a flapping check would
    // otherwise break byte-identity on a box nobody touched.
    let container = container_of("health-volatility", "limited");
    let health = field(&field(&container, "state"), "health");

    // Act & Assert
    assert_eq!(
        field(&container, "healthcheck").volatility(),
        Volatility::Stable
    );
    assert_eq!(health.volatility(), Volatility::Volatile);
    assert_eq!(text(&field(&health, "status")), "unhealthy");
    assert_eq!(integer(&field(&health, "failing_streak")), 2);
}

#[test]
fn the_health_log_is_not_recorded_at_all() {
    // Arrange: the log is the output of the check's own command, and a failing database
    // check prints its connection error, credentials and all. It is also a rolling window
    // that changes on every run. There is no reading of it that belongs in a fingerprint,
    // so it is the one field here that is dropped rather than annotated.
    let health = field(
        &field(&container_of("health-log", "limited"), "state"),
        "health",
    );

    // Act & Assert
    assert_eq!(
        keys_of(&health),
        vec!["failing_streak".to_owned(), "status".to_owned()]
    );
}

#[test]
fn the_log_driver_and_its_options_are_recorded() {
    // Arrange: where a container's output goes, and whether it is bounded. An unbounded
    // json-file driver is how a box fills its disk, so the options are as much state as the
    // driver.
    let logging = field(&container_of("logging", "limited"), "logging");

    // Act & Assert
    assert_eq!(text(&field(&logging, "driver")), "json-file");
    assert_eq!(text(&field(&field(&logging, "options"), "max-size")), "1m");
    assert_eq!(text(&field(&field(&logging, "options"), "max-file")), "3");
}

#[test]
fn a_container_on_the_engines_default_logging_records_no_options() {
    // Arrange
    let logging = field(&container_of("default-logging", "web"), "logging");

    // Act & Assert
    assert_eq!(text(&field(&logging, "driver")), "json-file");
    assert!(keys_of(&field(&logging, "options")).is_empty());
}

fn image_of(name: &str, id: &str) -> Observation {
    field(&field(&answering_server(name), "images"), id)
}

#[test]
fn the_images_are_keyed_by_their_own_id() {
    // Arrange: the opposite arrangement from containers, and for the opposite reason. A
    // container's name outlives its id; an image's tags are the thing that moves.
    let images = field(&answering_server("images"), "images");

    // Act & Assert
    assert_eq!(
        keys_of(&images),
        vec![DANGLING_IMAGE.to_owned(), TAGGED_IMAGE.to_owned()]
    );
}

#[test]
fn an_image_records_every_tag_that_points_at_it_sorted() {
    // Arrange: two tags on one image is ordinary, `:1` and `:latest` being the usual pair,
    // and docker promises no order between them.
    let image = image_of("image-tags", TAGGED_IMAGE);

    // Act & Assert
    assert_eq!(
        items_of(&field(&image, "tags"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        vec!["fixture-app:1".to_owned(), "fixture-app:latest".to_owned()]
    );
    assert!(items_of(&field(&image, "registry_digests")).is_empty());
}

#[test]
fn an_image_records_its_size_and_the_platform_it_was_built_for() {
    // Arrange: a multi-architecture tag hides the platform, and an image whose architecture
    // does not match the box is a container that will not start.
    let image = image_of("image-platform", TAGGED_IMAGE);
    let platform = field(&image, "platform");

    // Act & Assert
    assert_eq!(integer(&field(&image, "size_bytes")), 8_652_792);
    assert_eq!(text(&field(&platform, "architecture")), "arm64");
    assert_eq!(text(&field(&platform, "operating_system")), "linux");
    assert_eq!(text(&field(&platform, "variant")), "v8");
}

#[test]
fn a_dangling_image_records_no_tags_rather_than_being_dropped() {
    // Arrange: the image a rebuild displaced. It holds disk, it is usually an accident, and
    // `<none>:<none>` in `docker images` is the only place an operator ever sees it.
    let image = image_of("dangling", DANGLING_IMAGE);

    // Act & Assert
    assert!(items_of(&field(&image, "tags")).is_empty());
    assert_eq!(integer(&field(&image, "size_bytes")), 8_652_792);
}

#[test]
fn an_images_labels_carry_the_provenance_of_the_build() {
    // Arrange: `org.opencontainers.image.revision` names the commit an image was built
    // from, which is the only link from a running box back to the source that made it.
    let labels = field(&image_of("provenance", DANGLING_IMAGE), "labels");

    // Act & Assert
    assert_eq!(
        text(&field(&labels, "org.opencontainers.image.revision")),
        "9f8e7d6"
    );
    assert_eq!(
        text(&field(&labels, "org.opencontainers.image.title")),
        "fixture"
    );
}

#[test]
fn an_image_the_engine_still_knows_the_parent_of_records_it() {
    // Arrange: the classic builder records a parent chain and buildkit records none, so
    // both a digest and an absence are ordinary here.
    let image = image_of("image-parent", TAGGED_IMAGE);

    // Act & Assert
    assert_eq!(
        text(&field(&image, "parent")),
        "sha256:d0e93b62e38199f58d210648e86e485c13900f798f372a2cf31032a904c6f232"
    );
}

#[test]
fn an_image_that_vanished_while_being_read_is_recorded_too() {
    // Arrange: `docker image prune` does to images exactly what a cron `--rm` does to
    // containers, so the same loss list serves both.
    let server = field(
        &field(
            &docker_facet(
                "vanished-image",
                DockerFixtures {
                    version: VERSION_ANSWERING,
                    version_stderr: "",
                    info: INFO_ANSWERING,
                    containers: &[],
                    images: &[
                        (TAGGED_IMAGE, Some(INSPECT_TAGGED_IMAGE)),
                        (DANGLING_IMAGE, None),
                    ],
                    volumes: &[],
                    networks: &[],
                },
            ),
            "docker",
        ),
        "server",
    );
    let unreadable = field(&server, "unreadable_images");

    // Act & Assert
    assert_eq!(
        keys_of(&field(&server, "images")),
        vec![TAGGED_IMAGE.to_owned()]
    );
    assert_eq!(unreadable.volatility(), Volatility::Volatile);
    let entries = items_of(&unreadable);
    assert_eq!(entries.len(), 1);
    assert_eq!(text(&field(&entries[0], "id")), DANGLING_IMAGE);
    assert!(text(&field(&entries[0], "reason")).contains("No such image"));
}

fn volume_of(name: &str, volume: &str) -> Observation {
    field(&field(&answering_server(name), "volumes"), volume)
}

#[test]
fn the_volumes_are_keyed_by_name() {
    // Arrange: a volume's name is its identity to the engine and to every container that
    // mounts it, and unlike a container it is never minted afresh.
    let volumes = field(&answering_server("volumes"), "volumes");

    // Act & Assert
    assert_eq!(
        keys_of(&volumes),
        vec!["fixture-opts".to_owned(), "fixture-vol".to_owned()]
    );
}

#[test]
fn a_volume_records_its_driver_and_where_the_engine_keeps_it() {
    // Arrange
    let volume = volume_of("volume-driver", "fixture-vol");

    // Act & Assert
    assert_eq!(text(&field(&volume, "driver")), "local");
    assert_eq!(
        text(&field(&volume, "mountpoint")),
        "/var/lib/docker/volumes/fixture-vol/_data"
    );
    assert_eq!(text(&field(&volume, "scope")), "local");
    assert_eq!(text(&field(&volume, "created")), "2026-09-08T11:45:17Z");
}

#[test]
fn a_volumes_driver_options_are_recorded_because_they_say_where_the_data_is() {
    // Arrange: the mountpoint of a `local` volume with `type=tmpfs` is a path the data is
    // not durably at, and for an NFS volume the options name the server holding it. Reading
    // the mountpoint alone would describe the wrong place with confidence.
    let volume = volume_of("volume-options", "fixture-opts");
    let options = field(&volume, "options");

    // Act & Assert
    assert_eq!(text(&field(&options, "type")), "tmpfs");
    assert_eq!(text(&field(&options, "device")), "tmpfs");
    assert_eq!(text(&field(&options, "o")), "size=32m");
    assert_eq!(
        text(&field(&field(&volume, "labels"), "com.example.owner")),
        "platform"
    );
}

#[test]
fn a_volume_created_with_nothing_but_a_name_records_neither_map() {
    // Arrange: docker writes null for both `Labels` and `Options`, and an empty map says
    // the same thing without claiming either was set to nothing.
    let volume = volume_of("volume-plain", "fixture-vol");

    // Act & Assert
    assert!(keys_of(&field(&volume, "options")).is_empty());
    assert!(keys_of(&field(&volume, "labels")).is_empty());
}

fn network_of(name: &str, network: &str) -> Observation {
    field(&field(&answering_server(name), "networks"), network)
}

#[test]
fn the_networks_of_the_box_are_keyed_by_name() {
    // Arrange: keyed by name, like a container's own view of them, so the two ends of one
    // network are read under the same word.
    let networks = field(&answering_server("box-networks"), "networks");

    // Act & Assert
    assert_eq!(
        keys_of(&networks),
        vec!["bridge".to_owned(), "fixture-net".to_owned()]
    );
}

#[test]
fn a_networks_addressing_is_recorded_as_the_engine_resolved_it() {
    // Arrange: the subnet is what a container's address has to fall inside, and the gateway
    // is the route out. Both shapes are real on one docker: a network created with a subnet
    // alone reported no gateway here immediately after creation and did report one after the
    // daemon restarted, so an absent gateway means unreported rather than none.
    let bridge = network_of("network-ipam", "bridge");
    let ipam = field(&bridge, "ipam");
    let configured = items_of(&field(&ipam, "configured"));

    // Act & Assert
    assert_eq!(text(&field(&ipam, "driver")), "default");
    assert_eq!(text(&field(&configured[0], "subnet")), "172.17.0.0/16");
    assert_eq!(text(&field(&configured[0], "gateway")), "172.17.0.1");
    assert!(is_null(&field(
        &items_of(&field(
            &field(&network_of("network-ipam", "fixture-net"), "ipam"),
            "configured"
        ))[0],
        "gateway"
    )));
}

#[test]
fn a_networks_driver_options_are_recorded_because_they_decide_what_it_permits() {
    // Arrange: `enable_icc` says whether containers on this network can reach each other,
    // `host_binding_ipv4` is the address an unqualified `-p` publishes to, and `name` is the
    // host interface the network actually is. All three are one line each and invisible
    // anywhere else in the document.
    let options = field(&network_of("network-options", "bridge"), "options");

    // Act & Assert
    assert_eq!(
        text(&field(&options, "com.docker.network.bridge.enable_icc")),
        "true"
    );
    assert_eq!(
        text(&field(
            &options,
            "com.docker.network.bridge.host_binding_ipv4"
        )),
        "0.0.0.0"
    );
    assert_eq!(
        text(&field(&options, "com.docker.network.bridge.name")),
        "docker0"
    );
}

#[test]
fn a_network_records_what_it_permits_beyond_its_addressing() {
    // Arrange: an `internal` network has no route off the box, and an `attachable` swarm
    // network lets a standalone container join it. Both are one word that changes what can
    // reach what.
    let network = network_of("network-flags", "fixture-net");

    // Act & Assert
    assert_eq!(text(&field(&network, "driver")), "bridge");
    assert_eq!(text(&field(&network, "scope")), "local");
    assert!(!boolean(&field(&network, "internal")));
    assert!(!boolean(&field(&network, "attachable")));
    assert!(!boolean(&field(&network, "ipv6_enabled")));
    assert_eq!(text(&field(&network, "id")), FIXTURE_NETWORK_ID);
}

#[test]
fn the_containers_attached_to_a_network_are_not_recorded_twice() {
    // Arrange: `docker network inspect` lists them, and every one of those containers
    // already records the network from its own end. Recording the same edge twice would
    // give a reader two places to disagree, and the container's end carries more.
    let network = network_of("network-edges", "fixture-net");

    // Act & Assert
    assert!(!keys_of(&network).contains(&"containers".to_owned()));
}
