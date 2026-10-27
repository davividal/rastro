# redis collector: setup matrix

Five cells, one per shape every box with redis is likely to be in. The matrix is for running
rastro against: a cell that reads right needs no test of its own, and one that reads wrong gets a
test for that defect alone. Each cell was captured on a real server, and its `/proc` side and
replies are the fixtures the tests read.

Every cell is read as root. **Unprivileged, no instance is read**: the server's `fd` is refused to
another account, so which sockets it holds cannot be told and the instance says so.

## Supported releases

From endoflife.date's API, rastro's source of truth for what to support, fetched 2026-10-06, with
one override by the maintainer:

- **redis 8.0, 8.2, 8.4, 8.6, 8.8 and 8.10.** endoflife.date also lists 7.4, 7.2 and 6.2 as
  maintained; they are best effort here, by the maintainer's choice.
- **valkey 7.2, 8.0, 8.1, 9.0 and 9.1**, every line endoflife.date lists as maintained.
- **Everything else is best effort**: read with the same rules, marked `unsupported`, and counted
  in the run's summary on stderr. The family and release come from `INFO server`: valkey's own
  `valkey_version`, since its `redis_version` is frozen at 7.2.4.

## Cells

| # | Release | Launch | Listening | Measured |
|---|---|---|---|---|
| 1 | redis 8.10.2 | redis.io deb and `redis-server.service`, every default | `127.0.0.1 -::1`, 6379 | ok; 303 settings, five modules |
| 12 | redis 8.10.2 | the `redis-server@<name>` template, `@queue` and `@cache`, `@cache` a replica of `@queue` | 6380, 6381 | ok; two instances, roles `master` and `slave` |
| 24 | redis 8.10.2 | docker official image, `-p 6379:6379`, a named volume | its own namespace, docker-proxy on the host | ok as root, through the namespace join; docker-proxy's host socket is not taken for the server's; the server's files in the volume's host directory sealed |
| 26 | valkey 9.1.2 | docker official image, every default | its own namespace | ok; `server: valkey`, the release from `valkey_version` |
| 33 | redis 7.4.11 | redis.io deb and its unit, every default | `127.0.0.1 -::1`, 6379 | read in full, `unsupported` |

valkey is captured from its image because Debian 12, the VM's release, packages no valkey.

## Not in the matrix

- **A password**: rastro sends a server that wants one nothing more, and says so.
- **A unix socket alone**: no packaged default listens only on one, and rastro dials TCP alone.
- **TLS, cluster mode, sentinels, renamed commands, modules loaded at runtime**: each is read by
  rules the fake-server tests cover, and none changes how a server is found or reached.

## How it is captured

The matrix tooling is kept in rastro-research (`collectors/redis-expert/matrix/`), run on the
Debian 12 VM `scripts/elasticsearch-matrix/Vagrantfile` makes: `cells.sh prepare` once, then
`run-all.sh` for one cell at a time, and `export-fixtures.sh` copies each cell into
`crates/rastro/tests/fixtures/redis/cells`.
