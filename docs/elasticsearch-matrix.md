# Elasticsearch collector: setup matrix

Not the cross product. One cell per shape a box is likely to be in, chosen so every axis
value appears at least once and every envelope outcome (`ok`, `not_read`, `unsupported`,
`error`, `absent`) is produced by at least one real node.

Every cell is read as root. **Unprivileged, every node is `not_read`**: the capture of every cell
shows `environ`, `fd` and `/proc/<pid>/root` refused to another account, so neither the node's
install, its config directory nor its listeners can be read. That outcome is the same for every cell,
which is why it is stated once here rather than as a column.

## Axes

| Axis | Values in the matrix |
|---|---|
| Version | 6.8, 7.10, 7.17, 8.15, 8.19, 9.2, 9.4, 9.5 |
| Launch | deb + systemd, tar.gz + `-d -p`, tar.gz foreground under a systemd unit, docker (official image) |
| Config | package default, `ES_PATH_CONF` elsewhere, file is a symlink, bind-mounted dir, env only (docker) |
| `-E` | none, visible (foreground), invisible (daemonised 8.19+), contradicting the file |
| Data | default, separate partition (LVM `/data`), two data paths (7.x only), docker named volume, docker bind mount |
| Network | default, `network.host: 0.0.0.0`, custom `http.port`, two nodes in one netns |
| Security | off, on + TLS (8.x and 9.x default), on without TLS, audit on |
| Credential | none, API key from a file, username and password from stdin, rejected |
| Cluster | single node, two-node cluster on one box, node not joined |

## Cells

| # | Version | Launch | Config | `-E` | Data | Network | Security | Expected |
|---|---|---|---|---|---|---|---|---|
| 1 | 7.17 | deb + systemd | default | none | default | default | off (7.17 default) | ok |
| 2 | 8.19 | deb + systemd | default | none | default | default | on + TLS (package default), no credential | ok, API side `not_read: no credential` |
| 3 | 8.19 | deb + systemd | default, security off | none | LVM `/data` | `0.0.0.0` | off | ok, `/data/...` sealed |
| 4 | 9.5 | deb + systemd | `ES_PATH_CONF=/srv/es/config` | none | default | custom `http.port` | off | ok |
| 5 | 9.4 | tar.gz `-d -p` | default under the tarball | invisible | default | default | off | ok, settings from the node |
| 6 | 8.19 | tar.gz `-d -p` | file says security off | invisible, turns TLS on | default | default | TLS via `-E` | `error` after one plain `GET /` (the blind spot) |
| 7 | 7.17 | tar.gz `-d -p` | default under the tarball | on the server argv | two data paths | default | off | ok, both paths sealed |
| 8 | 9.5 | tar.gz foreground, own systemd unit | symlinked file | visible on the launcher | LVM `/data` | default | off | ok |
| 9 | 9.5 | docker | env only | none | named volume | default | off | ok, volume's host dir sealed |
| 10 | 8.19 | docker | bind-mounted dir | none | bind mount on `/data` | default | off | ok |
| 11 | 9.4 | docker | symlinked file (ConfigMap shape) | `eswrapper -E` | container fs | custom `http.port` | off | ok |
| 12 | 8.19 | docker | defaults | none | container fs | default | on + TLS (image default), API key from a file | ok, read over TLS |
| 13 | 9.5 | docker, two containers | env | none | two volumes | own netns each | off | ok, one cluster, two nodes |
| 14 | 7.17 | tar.gz, two nodes on the host | default | visible | separate dirs | 9200 and 9201 in one netns | off | ok, each node its own port |
| 15 | 8.19 | docker | env | none | container fs | default | off, `cluster.initial_master_nodes` names an absent node | ok: node-local reads, cluster surfaces reported as "no master" |
| 16 | 6.8 | docker (amd64 under qemu) | env | none | container fs | default | off | ok from `/proc`, API side `not_read: below 7` |
| 17 | 7.10 | docker (oss image) | env | none | container fs | default | off | ok + unsupported, read as 7.17 |
| 18 | 8.15 | deb + systemd | security off | none | default | default | off | ok + unsupported, read as 8.19 |
| 19 | 9.2 | docker | env | none | container fs | default | off | ok + unsupported, read as 9.4; which launcher 9.2 uses is measured here |
| 20 | 9.5 | deb, installed, service stopped | default | none | default | default | off | ok, no nodes, `package_installed` true |
| 21 | 8.19 + 9.5 | deb + systemd, and docker | default each | none | default each | 9200 on the host, 9200 in the container | on + TLS each, a different API key each | the key's own cluster read, the other `not_read: credential rejected` (v1 limitation) |
| 22 | 9.5 × 2 | docker, two separate clusters | env, both left at the default `cluster.name` (`docker-cluster`) | none | volume each | own netns each | on + TLS each, a different API key each | the key's own cluster read, the other `not_read: credential rejected` (v1 limitation) |
| 23 | 7.17 × 2 | tar.gz, two separate clusters on the host | default | visible | separate dirs | 9200 and 9201 in one netns | on, a different credential each | the key's own cluster read, the other `not_read: credential rejected` (v1 limitation) |
| 24 | 9.5 | docker | env | none | volume | default | on + TLS, a key from another cluster | ok, API side `not_read: credential rejected` |
| 25 | 9.4 | deb + systemd | security on, HTTP TLS off | none | default | default | on without TLS, username and password from stdin | ok, read over plain HTTP with basic auth |
| 26 | 8.19 | docker | env, audit on | none | container fs | default | on + TLS + audit, API key from a file | ok; the audit entries the run leaves are recorded in the capture |
| 27 | 8.19 | deb + systemd | security off by `xpack.security.enabled: false` alone, the TLS block left as auto-configured | none | default | default | off | ok, plain HTTP |
| 28 | 8.19 × 2 | tar.gz `-d`, one cluster, the master killed | default | invisible | separate dirs | 9200 and 9201 | off | ok for the survivor, cluster surfaces `not_read: no master`, nothing waited out |
| 29 | 8.19 | tar.gz `-d` | default | invisible, ports 8200 and 8300 | default | outside the default ranges | off | `error`, the accepted blind spot |
| 30 | 8.19 | deb + systemd | `client_authentication: required` | none | default | default | on + mutual TLS | ok, API side `not_read: client certificate` |
| 31 | 8.15 → 8.19 | deb upgraded under the running node, not restarted | default | none | default | default | off | ok, read as the running 8.15.3, `unsupported`, restart pending |
| 32 | none | nothing installed or running | | | | | | absent |

The deb cells cannot share a VM: the package owns `/etc/elasticsearch` and one version at a
time. They run one after another on the same VM (purge between them), or on one VM each.

## How it is captured

`scripts/elasticsearch-matrix/`: a Debian 12 VM from its `Vagrantfile`, `cells.sh prepare` once
to download every package and image, then `run-all.sh`, which takes one cell at a time
(reset the box, set the cell up, `capture.sh`, collect the node logs). `summary.sh` prints one
line per captured node; `export-fixtures.sh` copies the `/proc` side into
`crates/rastro/tests/fixtures/elasticsearch/cells`.

On Apple silicon the VM needs `arm64.nosve` on the kernel command line, which the
`Vagrantfile` sets: JDK 22, bundled with 7.17, dies with SIGILL on the SVE VirtualBox reports.

## Captured from every cell

What becomes the test fixtures, so a test reads what a real node produced and not what a
report said it would:

- for each Elasticsearch process and its parent: `cmdline`, `environ`, `stat`, `status`,
  `cwd`, `root`, `mountinfo`, `ns/net`, `net/tcp` and `net/tcp6`;
- the files rastro reads under the node's root;
- the answer to every request in the fixed list, on cells that are asked;
- rastro's facet, which after review becomes the expected output.

## Considered and left out

- rpm: the same files as deb, which the envelope says make no difference.
- Kubernetes/ECK: cell 11 is its config shape; the pod's namespaces are what docker gives.
- rootless podman: a user namespace in front of the node; a likely v2 cell.
- OpenSearch: not Elasticsearch, so absent by definition. A one-line negative test, not a cell.
- keystore-only settings, cgroup limits, JVM options: none changes what rastro reads now
  that settings come from the node.
