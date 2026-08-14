# Design

How rastro is put together, and which parts of it are contract rather than
implementation detail. The *why* behind each choice lives in
[decisions.md](decisions.md); the field research behind the state model lives in
[research.md](research.md).

## Overview

```
cli (fingerprint verb)
 └─ collector registry
     ├─ built-in collectors (Rust, behind the Collector trait)
     └─ exec collectors (external executables, discovered from a dir/config)
 └─ output pipeline
     ├─ redaction layer (hash sensitive values unless --raw)
     └─ canonical JSON serialiser (decorator seam for future formats)
```

## The three-layer state model

Host state decomposes into three layers. Only the third is service-specific, and
it is *derived* from what Layer 2 observed, never guessed.

**Layer 1, filesystem.** Agnostic, complete, no declaration required.

**Layer 2, kernel and OS runtime.** The set of non-file state surfaces is fixed
and short, and does not grow with the number of tenants on the box.

**Layer 3, service-internal state.** Discovered by dispatching on Layer 2
signals: a running unit, a listening socket, a container image.

Throughout Layer 3, and wherever a choice exists, rastro prefers the
**effective, resolved** state over reading configuration files. `nginx -T`
resolves every `include`; `systemctl show` resolves every drop-in; `sysctl -a`
reflects runtime rather than intent. The two failure modes are different and
both matter: a file that changed without changing meaning is noise, and a
meaning that changed without touching a file is the dangerous one that a
file-hashing tool is silent about.

## Collector contract

A collector, built-in or exec, produces exactly one **facet**:

- `name`: stable identifier (`fs`, `processes`, `nginx`, …).
- `category`: **metadata** or **state**. Metadata collectors describe the run and
  the box it ran on (`host`, `invocation`); state collectors describe what is on
  the host. Same contract, different placement in the document, and metadata
  collectors cannot be switched off.
- `presence()`: is my subject on this host? Answers `present`, `absent`, or
  `undetermined` with a reason. Three-valued because a collector that *cannot
  tell* must be able to say so rather than report a confident `absent`.
- `collect()`: an observation tree, called only once presence is established, so
  it never has to express absence. Every value carries its own **annotations**:
  - `volatile`: this value changes on its own between two runs of an unchanged
    host (PIDs, counters, uptimes). Annotating a node covers everything under it.
  - `sensitive`: this value must not be printed as it stands.

A collector never mentions facets: assembling one from its answers is the use
case's job, which keeps every adapter free of the document model it feeds.

Collectors classify; they never present. What to do about an annotation is a
presentation decision, and *which* values are in a document is a domain one:
the same annotated observations produce both views.

**Absence is state.** An enabled collector whose `presence()` finds no tenant is
recorded with status `absent`. It is never silently omitted. A collector that
could not tell is recorded as `error` with its reason, never as `absent`.

**Failure is loud.** A collector that fails is recorded with status `error` and
the run continues. Failures appear in the output, never only in a log.

A collector disabled in the config *is* omitted, with a WARN on stderr. That
omission is still visible in a diff, because the full effective config is part
of the envelope.

### Exec contract

The same contract across a process boundary. The executable emits one facet as
JSON on stdout and exits 0. Output is schema-validated on ingest. A non-zero
exit or invalid output means the facet is recorded as `error` and the run
continues.

## Built-in collectors, v1

### Layer 1: filesystem walker

The largest single build item. A native walk over an exclusion-based path set.
Defaults: `/etc`, `/usr/local`, `/opt`, `/root`, `/var/spool/cron` and the
systemd unit directories with full attributes and hashing; `/srv` with
attributes only.

Per entry: permissions, uid, gid, size, mtime, ctime, inode, link target, file
type, ACLs, xattrs, and sha256 (skippable per tree). Symlinks are first-class
entries, since an enablement symlink under `*.wants/` is precisely the kind of
change this tool exists to catch.

**Exclusions, never inclusions**, are the user-tunable scope surface. An
exclusion that is wrong produces noise; an inclusion that is wrong produces a
blind spot. rastro's own files are excluded by default through explicit config
entries.

### Layer 2: the fixed runtime list

Processes, listening sockets, established connections, systemd unit states and
timers, kernel modules, runtime sysctl, the nftables/iptables ruleset, mounts,
the dpkg package list, users and groups, container state.

Read from `/proc` or netlink where that is cheap, and shell out to the canonical
tool (`systemctl`, `nft`, `dpkg-query`) where parsing its output is more honest
than reimplementing its semantics.

### Layer 3 starters: nginx, postgres, docker

`nginx -T`; `pg_dumpall --globals-only` plus `SHOW ALL`; `docker inspect` plus
volumes and networks. Three tenants are enough to prove the detect-and-dispatch
pattern that exec-contract authors will copy.

## Configuration

**Planned, not yet built.** rastro currently runs with no config file at all.

rastro runs as `./rastro` or `./rastro --config=/path/to/config.toml`. Without
the flag it looks for `config.toml` beside the binary. With no config found at
all it fails loudly and points the operator at `--generate-config`.

## Output format, the real contract

Everything else is implementation detail. This is not.

**Envelope:** `schema_version`, `metadata[]`, `facets[]`, in that order. That is
all, because everything else is observed by a collector.

`metadata[]` holds the metadata collectors' facets, among them `invocation`,
which carries the rastro binary version and rastro's **full effective config**
resolved from defaults plus config file plus CLI flags. That is the tool's own
effective-config principle applied to itself: any two fingerprints are
comparable, and the diff shows when they were produced under different settings
or a different release.

**Facet:** `name`, `collector` (id and version), `status` (`ok` | `absent` |
`error`), and then `data` when the status is `ok`, or `error` when it is not.
That order is deliberate: a reader scans for the name.
Volatile values sit in `data` where the collector put them.

**Leaf values** are `null`, boolean, integer or text. Floating point is
deliberately excluded; see the
[decision](decisions.md#the-format-admits-no-floating-point-numbers).

### Two views

One document, two views. A view says *what is in* the document; the format says
what it looks like. They are independent axes, so every format renders either
view:

- **diffable** (the default): volatile values omitted entirely, subtrees
  included. This is the view the determinism contract is about.
- **complete** (`--include-volatile`): everything the collectors observed,
  volatile values included. Two such runs of an unchanged host will differ.

The flag is named for what it does, the view for what it is. `--complete` would
have argued for itself, since nobody wants an incomplete picture of their
server.

Diffable is the default because a default that produces noise teaches the
operator that the tool is noisy. Getting a cleanly diffable fingerprint must not
depend on knowing that a flag exists.

### Determinism rules

Contractual, and tested:

- a fixed key order in every object, which is *declared* where the shape is
  known (document, facet, collector) and *sorted* where it is not
  (whatever a collector observed);
- a defined ordering for every list;
- no map iteration order leaking into output;
- no floating point, so no float-formatting differences;
- volatile values excluded from the diffable view.

Where keys are sorted, that is structural rather than conventional: an open
shape goes through a `BTreeMap`, so its ordering is a property of the data
structure and not of anyone's discipline. Where keys are declared, the order is
part of the contract and a test reads it back out of the rendered bytes, because
parsing JSON into a map would sort it and hide any mistake.

A renderer added later, for YAML or anything else, inherits the same rules.
Canonicalisation is a constraint on the renderer interface, not a choice each
renderer makes.

Two runs on an unchanged box must produce a **byte-identical** diffable view.
This is the whole reason `diff(1)`, `dyff` and `jd` are sufficient and rastro
ships no diff engine.

### Streams

stdout carries the fingerprint and nothing else. Errors and warnings go to
stderr or a log file unless explicitly suppressed, so an operator can see what
went wrong without parsing JSON.

## Security posture

Of the following, only the `unsafe`-free build and the absence of network I/O
are true today. Redaction, the output-file mode and the root requirement arrive
with the collectors and the config layer that need them.

- **Requires root.** It reads `/etc`, user crontabs and firewall state.
  Degrading gracefully without root is roadmap, not v1.
- **Output file created `0600`.**
- **Redaction on by default**, `--raw` prints the warning it deserves.
  Redaction is a configuration option, not a guarantee: marking fields
  `sensitive` is the collector author's responsibility, and the exec contract,
  the docs, the tests and rastro's own output all say so explicitly.
- **No network I/O in v1.** A v1 simplification rather than project policy.
  A future collector may legitimately touch the network, for example a firewall
  collector verifying rules from outside the ruleset dump.

## Verification

**The determinism harness is the flagship test.** A full run twice in an idle
container, asserting the diffable sections are byte-identical. It catches
map-order leaks and unannotated volatile fields, which is to say the noise
floor, at CI time instead of on a production box. Any new collector or format
change must keep it green.

### Running in CI today

- The determinism harness above, as `crates/rastro/tests/cli.rs`, through the
  built binary.
- Per-collector unit tests against fixture inputs, so parsing is exercised
  without needing the host it parses (`crates/rastro/tests`).
- That neither library crate reads the host, and that `rastro-fingerprint`'s
  own modules form no cycle (`tests/purity.rs` in each).
- That inside `rastro` the collectors and the command line stay ignorant of each
  other, and that a collector reaches the document model only through the port
  (`crates/rastro/tests/purity.rs`). Those are siblings in one crate, so cargo
  draws no line between them.
- That a collector can be written against `rastro-collector` alone
  (`tests/one_dependency_is_enough.rs`), which is the crate's headline promise.
- `fmt`, `clippy` with warnings as errors, a build on the declared MSRV, and an
  assertion that the shipped musl binary really is statically linked.

The crate boundaries themselves are enforced by cargo, which will not compile a
cycle, so they need no test.

### Planned, not yet running

Listed so the gap is visible rather than implied:

- Integration runs on Debian (primary) and Ubuntu containers: run, validate
  against the schema, mutate one thing (add a user, enable a unit, `chmod` a
  file), re-run, and assert that the mutation and *only* the mutation appears in
  the diff.
- The two changes from the field research that motivated the attribute set, a
  permissions-only change and an enablement symlink, surfacing once the Layer 1
  walker exists.
- Noise-floor calibration documented in the README as a first-run ritual.
