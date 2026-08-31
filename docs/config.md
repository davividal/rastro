# Configuration

Optional. With no config, every collector runs. A config can only narrow that.

```sh
rastro --config /etc/rastro.toml
```

The path is always given; there is no auto-discovery.

## Format

```toml
[collectors]
exclude = ["mounts"]
```

| collector      | category | excludable |
| -------------- | -------- | ---------- |
| `host`         | metadata | no         |
| `invocation`   | metadata | no         |
| `accounts`     | state    | yes        |
| `block_devices`| state    | yes        |
| `cron`         | state    | yes        |
| `exporters`    | state    | yes        |
| `filesystem`   | state    | yes        |
| `firewall`     | state    | yes        |
| `locale`       | state    | yes        |
| `modules`      | state    | yes        |
| `mounts`       | state    | yes        |
| `network`      | state    | yes        |
| `packages`     | state    | yes        |
| `postgresql`   | state    | yes        |
| `processes`    | state    | yes        |
| `repositories` | state    | yes        |
| `sockets`      | state    | yes        |
| `ssh_access`   | state    | yes        |
| `sysctl`       | state    | yes        |
| `time`         | state    | yes        |
| `timers`       | state    | yes        |
| `units`        | state    | yes        |

Metadata collectors cannot be excluded: without them one fingerprint cannot be
told apart from another.

## What exclusion does

The facet is omitted from the document, not recorded `absent`, and a line goes
to stderr. The exclusion itself appears in the `invocation` facet, alongside the
view, because both change what is in the document:

```json
"config": {
  "detail": "summary",
  "excluded_collectors": ["mounts"],
  "source": "/etc/rastro.toml",
  "staged_binary": false,
  "view": "diffable"
}
```

so two runs under different scope cannot be diffed without the difference
showing. `source` is `null` when no `--config` was given, and a path that is not
valid UTF-8 is refused rather than recorded lossily.

Exclusions are sorted and deduplicated, so two configs meaning the same thing
produce the same document.

There is no include list. What decides it is which way each kind of list fails
when you forget an entry, and forgetting is the likely mistake on a box nobody
documented.

Forget to exclude a collector and the fingerprint carries a surface you did not
want: noise, visible in the diff, and you exclude it on the next run. Forget to
include one and a whole state surface is missing, with nothing in the output
saying so. Even an exclusion made by mistake stays discoverable, because
`excluded_collectors` is in the `invocation` facet, so a diff shows the scope
changed. A missing inclusion leaves no trace at all.

## Narrowing the walk

The filesystem walk reads every mount that holds files, and a config can tell it to step back
from trees the operator knows are noise. Three keys, and all three withhold:

```toml
[filesystem]
# Stat these, open nothing in them.
metadata_only = ["/srv/media"]
# And treat what moves in them as moving on its own: size, inode and both stamps go volatile.
churns = ["/home/runner/actions-runner"]
# Record the tree's own directory and do not descend.
sealed = ["/var/lib/mysql", "/usr/share/doc"]
```

Measured on the reference box, that last one alone took 3,028 entries out of the document.

**There is deliberately no `hashed` key.** A config may only narrow, and asking for content to
be read would widen the walk — which is the inclusion list this tool exists to refuse. An
unknown key is an error, so an attempt at one fails rather than quietly doing nothing.

**The operator's rule beats a collector's claim.** A collector that owns a tree claims it from
the host — `postgresql` seals each cluster's data directory, `packages` churns the package
database. That is rastro's reckoning about a tree from the outside; the operator knows their
box, so naming the same tree in a config replaces the claim rather than conflicting with it.
Naming one tree twice *in the config* is still an error: the operator meant one of them and
rastro cannot know which.

**Every rule appears in the `invocation` facet**, keyed by tree, with the reading and who asked
for it — `filesystem` for a shipped rule, a collector's name for a claim, `config` for one of
these. A tree with no entries under it owes the reader that much:

```json
"walk_policy": {
  "/": { "claimed_by": "filesystem", "reading": "metadata_only" },
  "/usr/share/doc": { "claimed_by": "config", "reading": "sealed" },
  "/var/lib/postgresql/17/main": { "claimed_by": "config", "reading": "sealed" }
}
```

## What a config cannot do

**Where the document goes is not configurable.** A config may only *narrow* a run, and
choosing an output path narrows nothing, so `-o` is a command-line option and only that.
