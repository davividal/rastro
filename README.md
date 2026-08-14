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
server, no SSH, no fleet. One box, one invocation, one document.

No `diff` verb either: the format is contractually diffable, so `diff(1)`,
`dyff` or `jd` are enough. See
[the format contract](docs/design.md#output-format-the-real-contract).

## Building

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all

rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

The musl target is what ships: one static binary for a host with nothing
installed on it. CI asserts it really is static.

## Documentation

| document                               | contents                                          |
| -------------------------------------- | ------------------------------------------------- |
| [docs/design.md](docs/design.md)       | architecture, collector contract, output format   |
| [docs/decisions.md](docs/decisions.md) | what was chosen, why, and what it costs           |
| [docs/research.md](docs/research.md)   | prior art, and why the alternatives were rejected |

## Licence

AGPL-3.0-only, see [LICENSE](LICENSE). Contributions under DCO sign-off, no CLA.
[Why](docs/decisions.md#licence-agpl-30-only).
