# redis collector: setup matrix

Not the cross product. One cell per shape a box is likely to be in, chosen so every axis value
appears at least once and every outcome the facet reports is produced by at least one real
server. Each cell was captured on a real server and its `/proc` side, units, files and replies are
the fixtures the tests read, so a test reads what a server produced and not what a report said it
would.

Every cell is read as root. **Unprivileged, no instance is read**: the capture of every cell shows
the server's `fd` refused to another account, so which sockets it holds cannot be told and the
instance says so. That outcome is the same for every cell, which is why it is stated once here
rather than as a column.

## Supported releases

From endoflife.date's API, rastro's source of truth for what to support, fetched 2026-10-06, with
one override by the maintainer:

- **redis 8.0, 8.2, 8.4, 8.6, 8.8 and 8.10.** endoflife.date also lists 7.4, 7.2 and 6.2 as
  maintained; they are best effort here, by the maintainer's choice.
- **valkey 7.2, 8.0, 8.1, 9.0 and 9.1**, every line endoflife.date lists as maintained.
- **Everything else is best effort**: read with the same rules, marked `unsupported`, and counted
  in the run's summary on stderr. The family and release come from `INFO server`: valkey's own
  `valkey_version`, since its `redis_version` is frozen at 7.2.4.

## Axes

| Axis | Values in the matrix |
|---|---|
| Family and release, supported | redis 8.0, 8.2, 8.4, 8.6, 8.8, 8.10; valkey 7.2, 8.0, 8.1, 9.0, 9.1 |
| Family and release, best effort | redis 7.4, 7.0 (Debian 12's own package), 6.2, 5.0 (the field host) |
| Launch | redis.io deb and `redis-server.service`; the `redis-server@<name>` template; Debian 12's own deb; a source build under its own unit; started by hand from a shell; docker official image |
| Config | package default; a drop-in `include` glob; file outside `/etc/redis`; `CONFIG SET` only; after `CONFIG REWRITE`; `rename-command CONFIG ""`; `set-proc-title no` |
| Listening | `127.0.0.1 -::1` (package default); `bind 0.0.0.0` (field host); custom port; unix socket only; `port` and `tls-port`; `tls-port` only; a container's own namespace, with and without a published port |
| Auth | none; `requirepass` in the file; `requirepass` by `CONFIG SET` only (estate); file after `CONFIG REWRITE`; that file edited by hand later; password rotated at runtime; `aclfile`; `--requirepass` on the unit's command line; `default` switched off |
| Data | `/var/lib/redis`; `dir` on a separate partition (LVM `/data`) with AOF on; `dir ./` from a home directory; docker volume; a container's own image |
| Topology | standalone; a replica of a second instance on the box; `cluster-enabled yes`; `redis-sentinel` alone |
| Modules | none; the redis.io build's bundled modules; one loaded at runtime with `MODULE LOAD` |
| Identity | `comm` matching the family; valkey started through a `redis-server` link, as Debian's compatibility package installs one |

## Cells

`ACL LOG` and the server's log were read before and after both of rastro's runs of every cell:
nothing rastro did added to either, except where the outcome says so.

### redis 8, supported

| # | Release | Launch | Config / data | Listening | Auth | Measured |
|---|---|---|---|---|---|---|
| 1 | 8.10.2 | redis.io deb + unit | default | default | none | ok, the baseline; 303 settings, five modules |
| 2 | 8.0.6 | redis.io deb + unit | default | default | none | ok, the oldest supported line |
| 3 | 8.2.10 | redis.io deb + unit | `maxmemory`, `save ""` by `CONFIG SET` | `bind 0.0.0.0` | `requirepass` in the file | ok after one `AUTH`; the field host and the estate on 8 |
| 4 | 8.4.7 | redis.io deb + unit | default | default | `requirepass` by `CONFIG SET` only | refused: set at runtime; nothing sent after `NOAUTH` |
| 5 | 8.6.7 | redis.io deb + unit | after `CONFIG REWRITE` | default | `requirepass` + generated `user default #…` | ok; the hash checked before `AUTH` |
| 6 | 8.6.7 | redis.io deb + unit | rewritten, then `requirepass` edited by hand | default | the two disagree | refused: no password matches the hash; nothing sent |
| 7 | 8.8.3 | redis.io deb + unit | default | default | `requirepass` rotated by `CONFIG SET` after start | refused by the server: changed since start; exactly one `ACL LOG` entry, the documented cost |
| 8 | 8.8.3 | redis.io deb + unit | `aclfile` with `default` and two more accounts | default | from the ACL file | ok; three accounts, verifiers redacted |
| 9 | 8.10.2 | redis.io deb + unit | `port 0` | unix socket only, mode 770 | none | ok, keyed by the path |
| 10 | 8.10.2 | redis.io deb + unit | TLS on 6379, plain on 6380 | `port` + `tls-port` | none | ok on 6380, from the title; nothing in the server's log |
| 11 | 8.10.2 | redis.io deb + unit | `port 0` | `tls-port` only | none | error naming the hang-up; one TLS line in the server's log, the documented cost |
| 12 | 8.10.2 | template, `@queue` and `@cache` | `@cache` a replica of `@queue` | 6380, 6381 | none | ok; two instances, roles `master` and `slave` |
| 13 | 8.10.2 | redis.io deb + unit | `cluster-enabled yes`, one node | default | none | ok; `mode: cluster`, no topology read |
| 14 | 8.10.2 | redis.io deb + unit | `rename-command CONFIG ""` | default | none | ok with `settings: null` and its reason; the rest read |
| 15 | 8.10.2 | redis.io deb + unit | `set-proc-title no`, `include` glob of a drop-in directory | 6390 | `requirepass` in the drop-in | ok; the password found through the glob, inside the server's root |
| 16 | 8.10.2 | redis.io deb + unit | `dir /data/redis` on LVM, AOF on | default | none | ok; `/data/redis` sealed, found through the mount tables |
| 17 | 8.10.2 | redis.io deb + unit | three modules from the file, a fourth by `MODULE LOAD` | default | none | ok; all five modules (`enable-module-command` had to allow the load) |
| 18 | 8.10.2 | redis.io deb + unit | default | default | `default` switched off, another account on | refused: the default account is off; nothing sent |
| 19 | 8.10.2 | by hand, from `/root` | `dir ./` | `bind *` | none | ok; `/root` sealed from the walk, see the open question below |
| 20 | 8.10.2 | by hand | default | `bind *` | `requirepass` | refused: no unit, nothing guessed |
| 21 | 8.10.2 | redis.io deb, service stopped | default | n/a | n/a | present, `installed: [redis]`, no instances |
| 22 | 8.10.2 | `redis-sentinel` alone | sentinel config | 26379 | none | present, `installed: [redis]` (the sentinel package installs the server), no instances |
| 23 | 8.10.2 | docker, no published port | env only | own netns | none | ok as root, through the namespace join; `/data` is the image's, not claimed |
| 24 | 8.10.2 | docker, `-p 6379:6379` | named volume | own netns, docker-proxy on the host | none | as 23; docker-proxy's host socket is not taken for the server's; the volume's host directory sealed |
| 25 | 8.8.3 | docker | config file bind-mounted | own netns | `requirepass` in that file | refused: no unit; the title names no file, and nothing on the host records the container's start command but the engine |

### valkey, supported

| # | Release | Launch | Config / data | Listening | Auth | Measured |
|---|---|---|---|---|---|---|
| 26 | 9.1.2 | docker | env only | own netns | none | ok; `server: valkey`, the release from `valkey_version` |
| 27 | 9.0.6 | source build, own unit | default | `bind 0.0.0.0` | `requirepass` in the file | ok after one `AUTH`; the field host's shape on valkey |
| 28 | 8.1.10 | source build, started through a `redis-server` link, own unit | default | default | none | ok; `comm` says redis, `server_name` makes it valkey |
| 29 | 8.0.11 | source build, own unit | `port 0` | unix socket only | `aclfile` | ok after one `AUTH`; accounts from the ACL file |
| 30 | 7.2.14 | docker | TLS on 6379, plain on 6380 | `port` + `tls-port`, own netns | none | ok on 6380 from the title, inside the namespace |
| 31 | 9.1.2 + redis 8.10.2 | docker and redis.io deb | default each | 6379 on the host, 6379 in the container | none | two instances, keyed by the lowest address each holds on the shared port |

### Best effort

| # | Release | Launch | Config / data | Listening | Auth | Measured |
|---|---|---|---|---|---|---|
| 32 | redis 7.0.15 | Debian 12's deb + unit | default | default | `requirepass` in the file | read in full after one `AUTH`, `unsupported` |
| 33 | redis 7.4.11 | redis.io deb + unit | default | default | none | read in full, `unsupported` |
| 34 | redis 6.2.24 | redis.io deb + unit | default | default | `--requirepass` on the unit's command line | read in full, `unsupported`; the command line outranks the file |
| 35 | redis 5.0.14 | source build, own unit | default | `bind 0.0.0.0` | `requirepass` in the file | read after one `AUTH`, `unsupported`; `acl: null`, nothing sent to find out |

The deb cells cannot share a box: the package owns `/etc/redis` and one version at a time. They
run one after another on the same VM, purged between them. The redis.io repository carries every
redis line from 6.0 to 8.10 for bookworm on arm64; 5.0 is not in it, and Debian 12 packages no
valkey, hence the source builds.

## Open questions

- **A server started by hand in a home directory** (cell 19). Its working directory is its `dir`,
  so `/root` is sealed from the walk to hide one dump file. The choice is between sealing, sealing
  only the dump and AOF names, and not claiming at all.

## How it is captured

The matrix tooling, kept in rastro-research (`collectors/redis-expert/matrix/`) rather than here,
on the Debian 12 VM `scripts/elasticsearch-matrix/Vagrantfile` makes:
`cells.sh prepare` once to add the redis.io repository, build valkey and redis 5 from source and
pull the images, then `run-all.sh`, which takes one cell at a time (reset the box, set the cell
up, `capture.sh`, collect the logs). `summary.sh` prints one line per captured server;
`export-fixtures.sh` copies each cell into `crates/rastro/tests/fixtures/redis/cells`. rastro
itself runs in the VM as a static build of the branch, so each cell also records what the
collector made of it.

## Captured from every cell

- for each server process: `cmdline`, `comm`, `stat`, `status`, `cgroup`, `mountinfo`, the `fd`
  list, the targets of `cwd`, `root`, `exe` and its namespace links, its own `net/tcp`, `net/tcp6`
  and `net/unix`, and which of them another account may read;
- the host's own socket tables, mount table and namespace links, which is what rastro compares
  each server's against;
- `systemctl show` for the unit the cgroup names, and the files the cell configured, read inside
  the server's own root at the same paths;
- the bytes each server sent in reply to rastro's fixed list of commands, without a credential and
  after `AUTH`;
- `ACL LOG` and the server's log before and after both of rastro's runs, so a cell proves what a
  read cost rather than asserting it;
- rastro's facet as root and unprivileged, its stderr and its exit status.

## Considered and left out

- Debian 13's own redis 8.0.2 and valkey 8.1.1 packages: the same files as cells 2 and 28's
  releases; the VM is Debian 12.
- rpm, snap and Homebrew: other file layouts, none of which changes what rastro reads now that
  values come from the server.
- redis-stack: superseded by redis 8's bundled modules.
- Kubernetes: the docker cells are its namespace shape.
- A unix socket inside a container: its path is the container's, and only its TCP sockets are
  dialled from the host. No cell found one in the wild.
- Sentinel and cluster topology reads: one box, not many.
