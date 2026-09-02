# Design

How rastro is put together, and which parts are contract rather than
implementation detail. The *why* is in [decisions.md](decisions.md); the field
research behind the state model is in [research.md](research.md).

## The three-layer state model

Only the third layer is service-specific, and it is *derived* from what Layer 2
observed, never guessed.

- **Layer 1, filesystem.** Agnostic, complete, no declaration required.
- **Layer 2, kernel and OS runtime.** A fixed, short set of state surfaces that
  does not grow with the number of tenants on the box.
- **Layer 3, service-internal.** Dispatched from Layer 2 signals: a running
  unit, a listening socket, a container image.

Wherever a choice exists, prefer **effective, resolved** state over reading
config files. `nginx -T` resolves every `include`; `sysctl -a` reflects runtime
rather than intent. Both failure modes matter: a file that changed without
changing meaning is noise, and a meaning that changed without touching a file is
the one a file-hashing tool is silent about.

## Collector contract

A collector, built-in or exec, produces exactly one **facet**:

- `name`: stable identifier (`fs`, `processes`, `nginx`, …).
- `category`: **metadata** or **state**. Metadata describes the run and the box;
  state describes what is on it. Same contract, different placement, and
  metadata collectors cannot be switched off.
- `presence()`: `present`, `absent`, or `undetermined` with a reason.
  Three-valued so a collector that *cannot tell* need not report a confident
  `absent`.
- `collect()`: an observation tree, called only once presence is established.
  Every value carries its own annotations:
  - `volatile` — changes on its own between two runs of an unchanged host.
    Annotating a node covers everything under it.
  - `sensitive` — must not be printed as it stands.

A collector never mentions facets; assembling one is the use case's job, which
keeps every adapter free of the document model it feeds.

**A built-in collector is layered**, and the arrows point one way: `source/` holds
one host interface's spelling, `model/` the types that render as a composed node,
`value_objects/` the types that render as a leaf. Nothing in the last two knows a
host interface exists, which is what makes a second interface reporting the same
concepts a new source rather than a change to the model. Enforced by
`crates/rastro/tests/purity.rs`. Shared value objects live in `rastro-collector`,
so an outside collector spells a path or a byte size the way the built-ins do.
See [decisions.md](decisions.md#a-collector-is-layered-source-model-value-objects).

**A collector observes and does not cause.** Reading the host must leave it as it
was found. This is not a matter of taste: a fingerprint of a box rastro has just
changed is not a fingerprint of that box, and the before-and-after pair an operator
takes around a change would report rastro's own footprint as the change's. Where the
richer interface costs a change and the poorer one does not, the poorer one wins and
the gap is documented. Where only a subsystem-specific tool will do, it is run only
when that subsystem is already resident — see
[decisions.md](decisions.md#rastro-does-not-change-the-host-it-describes).

**Absence is state.** No tenant means status `absent`, never a silent omission.
A collector that could not tell is `error` with its reason, never `absent`.

**Failure is loud.** A failed collector is `error` and the run continues.

An excluded collector *is* omitted, with a WARN on stderr. The omission stays
visible in a diff, because the effective config is in the envelope.

### Exec contract

The same contract across a process boundary: the executable emits one facet as
JSON on stdout and exits 0, schema-validated on ingest. A non-zero exit or
invalid output is recorded as `error` and the run continues.

## Built-in collectors, v1

**Layer 1, filesystem walker.** The largest build item. A native walk over
**every mount that holds files**, kernel interfaces aside: agnostic and complete,
because an operator cannot enumerate what nobody documented. Each mount is walked
separately and every walk stops at every mount point, so nothing is walked twice.
A collector may narrow the trees it owns; nothing may widen them.

Per entry, read: file type, permissions, uid, gid, size, mtime, ctime, inode,
link count, link target, device major and minor. Timestamps are nanoseconds since
the epoch, and there is no atime. Symlinks are first-class, since an enablement
symlink under `*.wants/` is exactly what this tool exists to catch.

Per entry, **recorded**: one digest of those attributes, which is what the default
view carries and answers the question a fingerprint is taken to answer — did
anything about this path change. `--detail` records all of them instead, and has to
be asked for at the time. ACLs and xattrs are still owed.

**Nothing is content-hashed.** Hashing every regular file read 84 GB on a
production host without finishing, and stat detects a change at any path anyway
because ctime has no userspace setter. Content hashing returns as an opt-in
collector over trees the operator names. See
[docs/decisions.md](decisions.md#metadata-everywhere-content-nowhere-by-default).

**Layer 2, the fixed runtime list.** Processes, listening sockets, established
connections, systemd units and timers, kernel modules, runtime sysctl, the
nftables/iptables ruleset, mounts, the package list, users and groups, container
state. A unit carries its effective `ExecStart=`, resolved by systemd rather than
read from the unit file, because "enabled and active" does not say which binary
that amounts to. Read from `/proc` or netlink where cheap, shell out to the canonical tool
where parsing its output is more honest than reimplementing it, and read a
manager's own database where the tool offers no format rastro controls. apk is
that case: it prints no machine-readable form, and every text form fuses name and
version into one token. The principle is to prefer the source that is unambiguous,
not to shell out on reflex.

Shelling out is confined to one hardened seam, `collectors::canonical_tool`:
absolute path, no shell, cleared environment, bounded in time and output, and a
breach kills the tool's whole process group. rastro runs as root on production, so
a collector must not be able to hang or flood the box it is inspecting. It returns
stdout, or both streams for the tools that answer on the wrong one — two of the six
telemetry agents print `--version` to stderr and exit zero.

**Layer 3 starters:** `nginx -T`; `pg_dumpall --globals-only` plus `SHOW ALL`;
`docker inspect` plus volumes and networks. Enough to prove the
detect-and-dispatch pattern exec-contract authors will copy.

**Layer 3, telemetry.** The agents watching the box — Prometheus-style exporters,
cAdvisor, collectd — as one `exporters` facet, keyed by the unit that starts each.
Dispatched from the **binary a unit starts**, matched against a named catalogue,
because a unit may be called anything and `process_exporter.service` runs
`process-exporter`. It earns its place against the package list: of the six agents
on the development box, `dpkg` has heard of one. The endpoint recorded is the one
the flags configure, which is a different fact from what `sockets` observes bound,
and the two are separate so they can disagree.

## Configuration

Optional, and it can only narrow a run. Exclusions only, so a config can never
hide a state surface the operator did not know to ask for.

Two things narrow: which collectors run, and how much of a tree the filesystem
walk reads — `metadata_only`, `churns`, `sealed`, and no `hashed`, because that
would widen it. An operator's rule over a tree beats a collector's claim to the
same tree, since a claim is rastro's reckoning from the outside and the operator
knows their box. Every rule is declared in the `invocation` facet with who asked
for it. Reference: [config.md](config.md).

## Output format, the real contract

Everything else is implementation detail. This is not.

**Envelope:** `schema_version`, `metadata[]`, `facets[]`, in that order. That is
all, because everything else is observed by a collector.

**Facet:** `name`, `collector` (id and version), `status`
(`ok` | `absent` | `error`), then `data` when `ok`, or `error` when not. Name
first, because that is what a reader scans for. Volatile values sit in `data`
where the collector put them.

**Leaf values** are `null`, boolean, integer or text. No floating point; see the
[decision](decisions.md#the-format-admits-no-floating-point-numbers).

### Two views

A view says *what is in* the document; the format says what it looks like. Two
independent axes, so every format renders either view.

- **diffable** (the default): volatile values omitted, subtrees included.
- **complete** (`--include-volatile`): everything observed. Two such runs of an
  unchanged host will differ.

Diffable is the default because a default that produces noise teaches the
operator that the tool is noisy.

### Determinism rules

Contractual, and tested:

- a fixed key order in every object: *declared* where the shape is known
  (document, facet, collector), *sorted* where it is not;
- a defined ordering for every list;
- no map iteration order leaking into output;
- no floating point, so no float-formatting differences;
- volatile values excluded from the diffable view.

Sorted keys are structural: an open shape goes through a `BTreeMap`. Declared
order is part of the contract, and a test reads it back out of the rendered
bytes, because parsing JSON into a map would sort it and hide any mistake.

Canonicalisation is a constraint on the renderer interface, not a choice each
renderer makes, so a YAML renderer inherits the same rules.

Two runs on an unchanged box must produce a **byte-identical** diffable view.
That is the whole reason `diff(1)`, `dyff` and `jd` are sufficient.

### Streams

The document goes to a file by default, `./rastro-<host>-<UTC>.json`, created
`0600`. `-o <path>` names another, `-o -` sends it to stdout. A megabyte-scale
document on a terminal punishes the first run, and a run killed before it printed
produced nothing at all.

stdout carries the fingerprint and nothing else — with the document in a file,
nothing at all. Errors, warnings, the live counter and `--debug` timings go to
stderr, and the counter only when stderr is a terminal, so a redirected clean run
still says nothing.

**Keep fingerprints off the walked tree.** A document written anywhere on real disk
is an entry of the next run's walk. rastro leaves out the file it is itself
writing, and declares that in the `invocation` facet, but a *timestamped* default
name means each run leaves a new file for the next one to find. Write to a fixed
path with `-o`, to a tmpfs, or off the box.

## Security posture

Of the following, the `unsafe`-free build, the absence of network I/O and the
output file's mode are true today. Redaction arrives with the collectors that need
it; the root requirement arrives with Layer 1.

- **Requires root.** It reads `/etc`, user crontabs and firewall state.
  Degrading gracefully without root is roadmap.
- **Output file created `0600`**, at creation rather than by a later `chmod`, so
  there is no window in which a document naming every path on the box is
  world-readable. Written to a temporary sibling and renamed, so a run that died
  half way leaves no half document to be diffed.
- **Redaction on by default**, `--raw` opts out with a warning. It is an option,
  not a guarantee: marking fields `sensitive` is the collector author's job.
- **No network I/O in v1.** A simplification, not policy — a firewall collector
  verifying rules from outside the ruleset dump would be legitimate.

## Verification

Collectors run on a pool of four, since most of a run is spent waiting for a tool
to answer. The filesystem walk runs alone afterwards, because it is the only
collector that could observe another one's side effects and report them once.

**The determinism harness is the flagship test:** a full run twice in an idle
container, diffable sections byte-identical. It catches map-order leaks and
unannotated volatile fields at CI time instead of on a production box.

### Running in CI today

- The determinism harness, through the built binary
  (`crates/rastro/tests/cli.rs`).
- Per-collector unit tests against fixture inputs, so parsing is exercised
  without the host it parses.
- That neither library crate reads the host, and that `rastro-fingerprint`'s
  modules form no cycle (`tests/purity.rs` in each).
- That inside `rastro` the collectors and the CLI stay ignorant of each other,
  and a collector reaches the model only through the port. Cargo draws no line
  between siblings in one crate.
- That a collector can be written against `rastro-collector` alone
  (`tests/one_dependency_is_enough.rs`).
- `fmt`, `clippy` as errors, `cargo doc` for intra-doc links, and an assertion that
  the shipped musl binary really is static. There is no MSRV job and no declared
  floor; `mise.toml` pins the toolchain and CI reads it.

Crate boundaries need no test: cargo will not compile a cycle.

### Planned, not yet running

- Integration runs on Debian and Ubuntu containers: mutate one thing, re-run,
  assert the mutation and *only* the mutation appears in the diff.
- The two field-research changes, a permissions-only change and an enablement
  symlink, once the Layer 1 walker exists.
- Noise-floor calibration as a documented first-run ritual.
