# rastro

`rastro` emits a canonical, diffable fingerprint of a host's state: filesystem,
OS runtime, and the internal state of the services it finds running.

One static binary, dropped on a Linux box you know nothing about, run as root.
It prints JSON and exits.

```sh
rastro > before.json
mount -t tmpfs -o ro,size=16m tmpfs /mnt/demo
# or ansible-playbook apply.yml / puppet apply / ./deploy.sh
rastro > after.json
diff -u before.json after.json
```

This is the whole output:

```diff
@@ -194,6 +194,18 @@
             "rw",
             "seclabel"
           ]
+        },
+        {
+          "device": "tmpfs",
+          "filesystem": "tmpfs",
+          "mount_point": "/mnt/demo",
+          "options": [
+            "inode64",
+            "relatime",
+            "ro",
+            "seclabel",
+            "size=16384k"
+          ]
         }
       ],
       "name": "mounts",
```

## Why

What a change _actually_ did to a live server, not what the tool that made it
claims.

Zero prior knowledge is the premise: if you could enumerate what changes, you
would not need the diff. Anything asking you to declare what to watch is
disqualified. [docs/research.md](docs/research.md) covers the alternatives, and
why AIDE and configsnap were rejected.

## What it is not

Not drift prevention, remediation, or monitoring. No agent, no daemon, no
server, no fleet. One box, one invocation, one document. The binary opens no
socket: [remote runs](#remote-hosts) are a shell wrapper around `ssh`, not a
client talking to a service.

No `diff` verb either: the format is contractually diffable, so `diff(1)`,
`dyff` or `jd` are enough. See
[the format contract](docs/design.md#output-format-the-real-contract).

## What it needs on the host

Nothing installed, and that is the point: one static binary, no runtime, no
library to match. Two exceptions, both about privilege rather than software.

**Root**, because `/etc`, user crontabs and the firewall ruleset are not
readable otherwise.

**`sudo`**, for the Layer 3 collectors that read a service as the account that
owns it. A PostgreSQL cluster with Debian's default `local all all peer` in
`pg_hba.conf` refuses root outright, so the cluster is reachable only as
`postgres`. rastro drops privilege to get there; it never gains any, which is
why this needs no sudoers entry.

Nothing in the binary needs `sudo` yet: no Layer 3 collector ships in
`built_in()` so far, so today's run wants root and nothing else. The rest of
this section is the contract the first one arrives under, written down before
it lands rather than after.

Where `sudo` is missing, or present and refusing, the facet will be an `error`
carrying the reason, never `absent`. A cluster listening on 5432 is running
whether or not rastro can reach it, so the only thing missing sudo establishes
is that rastro could not look. Reporting that as absence would put a confident
lie in the document, and the run continues either way.

Absence is still reported where it is genuinely knowable, and it needs no
privilege: no unit, no listening socket and no cluster directory means no
cluster, which is state.

## Remote hosts

`rastro` only ever fingerprints the box it runs on. To fingerprint another one,
push the binary over ssh, run it, delete it. That is the whole of what
[`rastro-ssh`](rastro-ssh) does, in a single connection:

```sh
./rastro-ssh ./rastro-aarch64 debian12 > before.json
```

The arguments are the binary to push and an ssh destination, followed by any
further options handed straight to `ssh`. Nothing is configured twice: the
destination is whatever your `~/.ssh/config` already resolves, keys, ports,
jump hosts and host-key checking included.

- The binary you name must match the target's architecture, because nothing
  probes it. `ssh <destination> uname -m` if you are unsure.
- `RASTRO_SSH_SUDO=1` prefixes the run with `sudo -n`. Unprivileged, the
  collectors that read `/etc/shadow`, user crontabs and the firewall ruleset
  report `error` rather than lying about the host.
- The binary is staged under `mktemp` in `/var/tmp`, mode 700, and removed by a
  trap that fires on a dropped link too. An interrupted run leaves nothing
  behind and needs no second login.
- The fingerprint reaches stdout only after a clean exit, so a run that died
  halfway cannot leave half a document to be diffed.
- It is produced by the same code as a local run, so the byte-identical
  guarantee survives the transport. A pty would not: the wrapper passes `-T`
  because CRLF translation would corrupt every line of it.
- The remote login shell has to be POSIX. csh and fish will not run the staging
  command.

One host per invocation, as ever. A fleet is a `for` loop, and `xargs -P` does
the parallel version better than rastro would.

## Building

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all

rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

The musl target is what ships: one static binary for a host with nothing
installed on it. CI asserts it really is static. Swap in
`aarch64-unknown-linux-musl` for an arm64 host.

## Documentation

| document                               | contents                                          |
| -------------------------------------- | ------------------------------------------------- |
| [docs/config.md](docs/config.md)       | the config file: format, options, refusals        |
| [docs/design.md](docs/design.md)       | architecture, collector contract, output format   |
| [docs/decisions.md](docs/decisions.md) | what was chosen, why, and what it costs           |
| [docs/research.md](docs/research.md)   | prior art, and why the alternatives were rejected |

## Licence

AGPL-3.0-only, see [LICENSE](LICENSE). Contributions under DCO sign-off, no CLA.
[Why](docs/decisions.md#licence-agpl-30-only).
