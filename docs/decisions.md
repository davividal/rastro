# Decision log

Decisions that are settled. Each entry states the choice, the reasoning, and
what it costs. Reversing one is a new entry, not an edit to an old one.

Entries are grouped by the work that produced them, and each group is dated where it begins.

| decision | choice |
| --- | --- |
| [Native collectors](#native-collectors-no-external-tool-as-a-dependency) | no AIDE, no configsnap; collectors are native |
| [Form](#form-single-static-rust-binary) | single static Rust binary, musl target |
| [Extensibility](#extensibility-collector-trait-plus-an-exec-contract) | built-in trait plus an exec contract, no recompile |
| [Audience](#audience-debiansystemd-first) | Debian/systemd first, generalisable without breaking changes |
| [v1 scope](#v1-scope-generate-only-current-box) | generate only, current box, stdout or file |
| [Output](#output-json-only-in-v1) | JSON only in v1, canonical, rendering as a decorator |
| [No diff verb](#no-diff-verb-in-v1) | out of v1; the format is contractually diffable instead |
| [Config](#configuration-is-mandatory) | mandatory config file. **Superseded** |
| [Config, revised](#superseded-config-is-optional-opt-in-and-exclusion-only) | optional, opt-in, exclusion-only; everything runs by default |
| [Secrets](#secrets-hashed-by-default) | sensitive values hashed by default, `--raw` opts out |
| [v1 collectors](#v1-collectors-layers-1-and-2-plus-three-layer-3-starters) | Layer 1 and Layer 2 complete, plus nginx, postgres, docker |
| [Licence](#licence-agpl-30-only) | AGPL-3.0-only |
| [Everything is a collector](#everything-observed-comes-from-a-collector) | host identity and invocation metadata are collectors, in a metadata category |
| [Annotations](#collectors-annotate-values-renderers-act-on-them) | collectors mark values volatile and sensitive; rendering decides what to do |
| [Volatile handling](#volatile-values-stay-in-place-the-diffable-view-omits-them) | in place, and omitted by the diffable view |
| [Diffable by default](#the-diffable-view-is-the-default) | `--include-volatile` opts into volatile values |
| [Key order](#keys-are-declared-where-the-shape-is-known-sorted-where-it-is-not) | fixed either way; readable where rastro owns the shape |
| [No floating point](#the-format-admits-no-floating-point-numbers) | leaf values are null, boolean, integer or text |
| [Presence is three-valued](#presence-is-three-valued-not-a-bool) | a collector that cannot tell says so, rather than reporting absence |
| [Layering](#layered-domain-application-infrastructure-presentation) | hexagonal, dependencies inward. **Superseded** |
| [Workspace](#superseded-a-cargo-workspace-replaces-the-hexagonal-layout) | three crates; cargo enforces what a test used to |
| [Layered collectors](#a-collector-is-layered-source-model-value-objects) | one host interface's spelling stays out of the model |
| [Keyed or listed](#keyed-where-the-name-is-unique-listed-where-it-is-not) | keyed loses nothing when names are unique, and removes ordering churn |
| [No load address](#a-modules-load-address-is-not-recorded) | a kernel text pointer is noise and a KASLR leak |
| [One execution seam](#shelling-out-goes-through-one-hardened-seam) | bounded, no shell, cleared environment, group-killed |
| [One packages collector](#one-packages-collector-dispatching-over-the-managers-it-finds) | keyed by manager; dpkg and apk read from different sources |
| [No MSRV floor](#no-msrv-floor-the-toolchain-is-pinned-by-mise) | only the latest Rust is maintained, and a floor constrained resolution |
| [Unclassified error text](#a-facets-error-text-is-not-classified-yet) | revisits when redaction exists to classify against |
| [Fingerprints are sensitive](#a-fingerprint-is-sensitive-operational-data-until-redaction-exists) | a package inventory is a target-selection aid, so handle the document accordingly |
| [Signalling by pid](#accepted-residual-risk-signalling-by-pid) | a pid-reuse window remains, accepted. **Superseded** |
| [Unconditional group signal](#superseded-the-group-signal-is-unconditional) | the guard that created the window was hiding the leak it was meant to guard |
| [What a unit starts](#a-unit-records-what-it-starts-resolved-by-systemd) | the `units` facet carries every unit's effective `ExecStart=` |
| [argv stays whole](#a-units-argument-vector-is-recorded-whole-not-split) | systemd loses the quoting, so splitting would invent a structure |
| [Exporters facet](#layer-3-a-telemetry-fleet-facet-dispatched-from-the-binary-a-unit-starts) | telemetry agents are read as their own facet, keyed by unit |
| [A named catalogue](#the-exporters-facet-knows-its-agents-by-name-rather-than-by-heuristic) | a heuristic would sweep in daemons and still miss two of six |
| [Both streams](#the-execution-seam-can-capture-stderr) | two of six agents print `--version` to stderr and exit zero |
| [Configured, not bound](#a-configured-endpoint-is-a-different-fact-from-a-bound-socket) | `exporters` and `sockets` observe separately so they can disagree |
| [Inode timestamps](#an-inodes-timestamps-are-nanoseconds-since-the-epoch-and-atime-is-not-recorded) | mtime and ctime in nanoseconds, neither volatile; no atime, since hashing moves it |
| [Device numbers](#a-device-node-records-its-major-and-minor-numbers) | a device node's state is its major and minor, split out of the packed `st_rdev` |
| [Hardened open](#hashing-opens-with-o_nofollow-and-o_nonblock) | the stat and the open are two calls, so the open refuses a swapped-in symlink or fifo |
| [Attributes owed](#acls-and-extended-attributes-are-owed-not-dropped) | ACLs and xattrs are one decision, deferred whole rather than built a third at a time |
| [pg_settings is one session's view](#pg_settings-is-one-sessions-view-not-the-clusters) | the facet compensates with the lens, role settings, and file settings rather than trusting it |
| [Running vs configured port](#a-clusters-running-port-comes-from-postmasterpid-its-configured-port-from-pg_lsclusters) | postmaster.pid is the observed half, kept apart from what pg_lsclusters configures |
| [Stable columns only](#only-the-stable-columns-of-a-moving-catalogue-are-read) | slots and pg_control give identity and shape, never the LSNs and counters that move |

## Native collectors, no external tool as a dependency

rastro reads state itself. AIDE and configsnap appear throughout
[docs/research.md](research.md) as evaluated candidates and were **rejected**:
AIDE covers only Layer 1 and needs fiddly configuration, configsnap is a
semi-abandoned scaffold with a verified silent-truncation bug and a
verified secret-handling bug.

They were research vehicles, not integration targets. rastro is not a frontend
or a driver harness for either. Leveraging an external tool is a nice-to-have at
most, never a dependency.

**Cost:** every collector is a build item, including a filesystem walker that
AIDE would otherwise have provided.

## Form: single static Rust binary

One statically linked binary targeting `x86_64-unknown-linux-musl` (and
aarch64). Copy it to the box, run it, delete it. No package, no install, no
runtime, no interpreter version to negotiate with a host nobody documented.

Rust for the static-linking story and for a filesystem walker that has to be
fast over tens of thousands of entries.

## Extensibility: collector trait plus an exec contract

Built-in collectors sit behind a `Collector` trait. Site-specific collectors are
external executables discovered from a directory, emitting one facet as JSON on
stdout. Adding a collector for an in-house service must never require
recompiling rastro.

## Audience: Debian/systemd first

The first target is a small Debian/systemd fleet operated by one person. Design
choices are allowed to be Debian-shaped where that buys simplicity, but must not
require a breaking change to generalise later. Anything that would paint the
format or the collector contract into a Debian-only corner is out.

## v1 scope: generate only, current box

Generate a fingerprint of the host rastro runs on, to stdout or a file. No
remote execution, no fleet aggregation, no history, no storage.

## Output: JSON only in v1

Canonical JSON, deterministically serialised. Rendering is a decorator over a
finished document, so YAML, XML or anything else can be added later with no
schema impact and no collector changes.

## No diff verb in v1

Comparing two fingerprints is the user's job, with the tools they already have.
This is affordable only because [the format is contractually
diffable](design.md#output-format-the-real-contract): a fixed key order, defined
list ordering, and a diffable view that omits volatile values, so two runs on an
unchanged box produce byte-identical output.

A generic structural `diff` verb is roadmap UX. It needs nothing from the
collectors, which is exactly why it can wait.

## Configuration is mandatory

rastro runs as `./rastro` or `./rastro --config=/path/to/config.toml`. Without
`--config` it looks for `config.toml` beside the binary. If no config is found
it fails loudly and points at `--generate-config`.

There is no implicit default configuration. The scope of what gets fingerprinted
is too consequential to inherit silently, and rastro's own files are excluded
through explicit config entries rather than built-in magic.

## Secrets: hashed by default

Values a collector marks `sensitive` are replaced by their digest at
serialisation time. `--raw` opts out and warns.

This is a configuration option, not a security guarantee. Marking fields is the
collector author's responsibility, and the exec contract, the documentation, the
tests and rastro's own output all say so.

## v1 collectors: Layers 1 and 2, plus three Layer 3 starters

Layer 1 (filesystem walker) and Layer 2 (the fixed OS-runtime list) are complete
in v1 because they are host-agnostic and bounded. Layer 3 ships with nginx,
postgres and docker only: enough to prove the detect-and-dispatch pattern and
give exec-contract authors a model to copy.

## Licence: AGPL-3.0-only

Chosen deliberately so the tool cannot be wrapped into a closed SaaS offering.

**Cost, accepted knowingly:** shops with a blanket AGPL ban will not adopt it.

Contributions under DCO sign-off rather than a CLA, which suits a solo
maintainer and keeps the barrier to a first patch low.

## Everything observed comes from a collector

Host identity and invocation metadata are not envelope fields written by the
binary. They are facets, produced by collectors, in a **metadata** category
alongside the **state** category that covers everything else.

Both categories share one contract: the same `ok | absent | error` outcomes, the
same annotations, the same rendering. The category decides where a facet lands
in the document, and that metadata collectors cannot be switched off, since a
run that failed to record which config was in effect is not a degraded
fingerprint but an uninterpretable one.

The gain beyond tidiness: the self-description travels the same path as
everything else, so every run exercises the collection and rendering machinery
end to end, including runs where every state collector came back `absent`.

`schema_version` is the one exception. It describes how to read the document,
including how to read the facets, so a collector cannot report it without the
reader having to parse the document to learn how to parse the document.

## Collectors annotate values, renderers act on them

Volatility and sensitivity are recorded per value, on any node of a collector's
output, not as separate payload sections. Nobody but the collector can tell a
self-changing value from a meaningful one, or a secret from a public fact, and
that judgement cannot be reconstructed downstream.

What to *do* about an annotation belongs to rendering: omit volatile values from
the diffable view, hash sensitive ones. Collectors classify; they never present.

Annotating a node covers everything under it, so a collector marks a whole
subtree volatile in one move rather than tagging every leaf.

## Volatile values stay in place, the diffable view omits them

The alternative was physical segregation into sibling `data` and `volatile`
objects. It was rejected: per-value volatility plus physical segregation
mirrors the structure, and for lists it produces two parallel arrays whose index
alignment silently carries meaning. That is a diff hazard precisely when a
fingerprint matters most.

So there are two *views* of one document, an axis independent of format. The
complete view carries everything observed. The diffable view drops volatile
values entirely, which makes "two runs on an unchanged host are byte-identical"
literally true rather than nearly true. Which values are in a view is a rule
about observations, so it lives in the domain; a renderer is told which view to
produce and never asks what volatility is.

**Accepted cost:** one document cannot be both complete and byte-stable, and the
complete view does not mark which values were volatile.

## The diffable view is the default

Running `rastro` with no arguments emits the diffable view. `--include-volatile`
opts into volatile values.

The flag is named for what it does rather than for the view it selects.
`--complete` would have argued for itself: nobody wants an incomplete picture of
their server, so it would read as the obvious choice rather than as the noisy
one.

The default is what almost everyone gets, what ends up in every write-up, and
what a hurried operator runs at three in the morning. If it emitted volatile
values, the first thing a new user would do is diff two runs, see PID and
timestamp churn scattered through the document, and conclude that rastro does
not work. Producing a cleanly diffable fingerprint must not depend on knowing
that a flag exists.

**Cost, accepted knowingly:** a fingerprint is a historical record, and volatile
values dropped from a "before" snapshot cannot be recovered afterwards, because
the state they described has moved on. That is acceptable only because volatile
is by definition the noise floor rather than meaningful state, and because
someone who wants to eyeball PIDs knows they are eyeballing.

**Closed by the config layer.** The chosen view is recorded in the `invocation`
facet's effective config, because a view is a flag and flags are part of it.
Diffing a complete document against a diffable one now shows `"view"` changing
at the top rather than pages of unexplained removals.

## Keys are declared where the shape is known, sorted where it is not

The document, facet and collector objects have a shape rastro owns, so their
keys are emitted in a declared order: `schema_version`, `metadata`, `facets` for
the document; `name`, `collector`, `status`, then `data` or `error` for a facet.
Whatever a collector observed has a shape rastro does not own, so those keys are
sorted.

Determinism needs a *fixed* order, and sorting is only one way to get one. It
was costing readability for nothing: it put `name` after `data`, so a facet did not
say what it was until after its payload. Sorting is the right answer only where
there is no declaration to follow.

**Cost:** the rule is two sentences instead of one, and the declared order now
depends on the order of statements in `presentation/canonical.rs` rather than on
a data structure. That is guarded by a test which reads the rendered bytes.
Asserting on a parsed document could not work: parsing JSON sorts every object,
so a parse-based order check passes whatever the renderer emitted. An earlier
version of that test was vacuous for exactly this reason.

## The format admits no floating point numbers

Leaf values are `null`, boolean, integer or text. Collectors with fractional
data emit a scaled integer (milliseconds, basis points) or text.

Rendering a float back to text is not reliably identical across platforms and
library versions, and consumers that read JSON through a language with one
numeric type will not round-trip it faithfully either. A byte-identical diffable
section is the contract the whole tool rests on, so this excludes a class of
determinism bug by construction rather than testing for it afterwards.

**Cost:** every collector author inherits the constraint, including exec-contract
authors, who will find it surprising until they read this entry.

## Presence is three-valued, not a bool

A collector answers `Present`, `Absent`, or `Undetermined { reason }`, and
`collect()` is called only in the first case, returning an observation or a
failure.

A bool cannot express the case that matters most. Asked whether postgres is on
the box, a collector whose `pg_isready` timed out cannot honestly say yes or no.
With a bool it returns `false`, and the fingerprint then states that postgres is
not installed. That is a confident lie recorded as real state, and it is exactly
the class of error rastro exists to eliminate, so the type must not permit it.

Two consequences follow. A collector never constructs a `FacetOutcome`, so no
adapter depends on the document model; the mapping from presence and collection
result to `ok | absent | error` lives in one place in the application layer,
where it is visible and tested. And "collection failed" stays distinct from "the
subject is not here", which a merged design would have let a collector conflate
by accident.

## Layered: domain, application, infrastructure, presentation

Dependencies point inward. `domain` depends on nothing; `application`,
`infrastructure` and `presentation` depend on `domain`; `main` is the
composition root and the only place that wires an adapter to a port.

| layer | holds |
| --- | --- |
| `domain` | what a fingerprint is, and its rules. No I/O, no JSON, no `/proc` |
| `application` | the one use case: fingerprint this host |
| `infrastructure` | the adapters that read the host, and the exec-collector bridge |
| `presentation` | rendering and the command line |

Two reasons specific to this tool, rather than a general preference for
architecture:

The collectors are almost entirely infrastructure. They read `/proc`, shell out
to `systemctl` and `nft`, and spawn external executables. Without a declared
boundary they end up beside the domain, and the model that has to stay
deterministic starts importing `std::process`.

The domain must be buildable and testable on a machine that is not the target
platform. Development happens on macOS; the target is Debian. Ports and adapters
is what makes "the whole model tests without a Linux host" a property of the
design rather than a coincidence that will lapse.

**Modules are named for the model, not for the types they hold.** Inside
`domain`, the modules are the model's joints: `fingerprint` (the document and
its consistency rules), `observation` (what was seen and what the seer asserted
about it), `collector` (who observes, under what contract). A `FacetName` lives
beside `Facet` because it names one, not in a file called `identifier` beside
every other newtype. Grouping by what a type *is* to the compiler rather than by
what it *means* leaves the module structure carrying no information about the
model, which is the failure mode a `domain/` directory name hides rather than
fixes.

**Deliberately not adopted: repositories and unit of work.** rastro persists
nothing, has no database, no transaction, and no aggregate to load by id. The
nearest thing is the output sink, which is a one-way write, not a repository.
Those blocks are load-bearing in the backend templates this layering is borrowed
from and would be empty ceremony here.

## Superseded: a Cargo workspace replaces the hexagonal layout

Supersedes [Layered](#layered-domain-application-infrastructure-presentation)
and the "single Rust crate" line in the repo-shape notes.

`src/domain`, `src/application`, `src/infrastructure` and `src/presentation`
became three crates under `crates/`:

| crate | holds | depends on |
| --- | --- | --- |
| `rastro-fingerprint` | what a fingerprint is, and its canonical JSON | nothing of ours |
| `rastro-collector` | the contract a collector fulfils, and how a set of them becomes a fingerprint | `rastro-fingerprint` |
| `rastro` | the tool: built-in collectors, CLI, wiring | both |

Two reasons, and the second is the one that decided it.

**The top level should say what the program is about.** A tree whose first level
reads `domain / application / infrastructure / presentation` announces the
pattern it was built with rather than the product. `fingerprint`, `collector`
and the tool itself say what rastro does.

**Boundaries the compiler enforces beat boundaries a test enforces.** The
layering rule used to be a set of assertions in `tests/layering.rs` reading the
source as text. It could be defeated by a `super::super::` path and it could
false-positive on a doc comment. Their disposition, one by one rather than by a
count.

One caveat on reading the left-hand column, because it caused a review to reach
the wrong conclusion once. That file is in **no git object**. It was staged
early, in a five-assertion form, and then grew two more during review before the
move deleted it from the working tree, so the version this table describes is
not recoverable and a reader cannot diff it. Recovering the staged blob and
concluding the last two rows were invented is the trap; the corroboration is
that the cycle assertion is what reported `collector -> fingerprint -> collector`
and caused the port split recorded below, which happened before the workspace
existed. The right-hand column is the checkable half: every test it names exists
today.

| assertion | now |
| --- | --- |
| domain depends on no other layer | refused by cargo; a crate cycle does not compile |
| application names no adapter | refused by cargo, same reason |
| domain reads nothing from the host | ported, one `tests/purity.rs` per library crate |
| domain modules form no cycle | ported into `rastro-fingerprint/tests/purity.rs` |
| the module graph matches a record | retired; the cycle walk carries the part that mattered |
| observations know nothing about documents | covered by that cycle walk, with the direction recorded in a comment |
| presentation and infrastructure do not know each other | **not** a crate boundary: `cli` and `collectors` are siblings in `rastro`, so it kept its own test |

It also settles the argument that produced the previous entry. The
`collector` to `fingerprint` cycle could be waved away inside one crate; across
crates it is a hard error, which forced the identity types into
`rastro-fingerprint` where a facet records them, and left `rastro-collector`
holding the port and the assembly that drives it.

**Cost:** three manifests instead of one, and version discipline between them
once anything is published. Contributors gain a shorter path: a new collector is
one file under `crates/rastro/src/collectors/`, its `mod` and `pub use` lines,
and an entry in `built_in()`, touching neither library crate. It needs one
dependency, `rastro-collector`, which is asserted rather than claimed.

**Not adopted:** bounded contexts. CLI and collectors are not separate domains;
`facet`, `observation` and `volatile` mean one thing everywhere. rastro is a
single bounded context, and the split above is modularity for contributors, not
strategic DDD.

## Superseded: config is optional, opt-in and exclusion-only

Supersedes [Configuration is mandatory](#configuration-is-mandatory).

`rastro` with no arguments collects everything. `--config <path>` can only
narrow that, and there is no way to say which collectors *do* run.

**The old entry contradicted the project's founding disqualifier.**
`docs/research.md` rules out any tool that "requires you to declare *what to
watch*", because if you could enumerate what changes you would not need the
diff. That is what disqualified AIDE and configsnap. Refusing to run without a
config file is the same disqualifier one level up.

It was also internally inconsistent with the project's own "exclusions, never
inclusions" (`CLAUDE.md`), which presupposes a default scope to exclude *from*.

**What the old entry was reaching for was never explicit input.** It is that a
fingerprint records what produced it, so two runs under different scope cannot
be diffed by accident. That is the envelope self-description invariant, and it
works better with defaults: the effective config reaches the `invocation` facet
whether it came from a file or from nothing at all. Explicitness belongs in the
output, not in the input, where it would fall on the one person who by
definition cannot supply it.

Three rules, each because the alternative is silent:

- an unknown collector name is an error: a typo'd `mount` would otherwise leave
  `mounts` running while the operator believed it was switched off;
- excluding a metadata collector is an error, since they cannot be switched off
  at all;
- an unknown key or table is an error, because a misspelled `excludes` that
  quietly does nothing is the same failure one level up.

No auto-discovery either. A `config.toml` picked up silently from beside the
binary is exactly how a stale file narrows a run and poisons a diff, so the
path is always given.

**Cost, accepted knowingly:** every built-in collector is opt-out rather than
opt-in, so a mistake in a new one runs on every box at the next release without
being asked for, and the bar for adding to `built_in()` rises accordingly.
Layer 3 collectors shell out (`nginx -T`, `pg_dumpall --globals-only`,
`docker inspect`), which means dropping the binary on a production box will
spawn those processes unasked. All are read-only and cheap, so this is accepted
for v1, to be revisited if a collector ever wants to do something genuinely
expensive.

---

The entries below date from the Layer 2 work, 2026-08-19.

## A collector is layered: source, model, value objects

Every built-in collector splits three ways, and the dependency arrows only point
one way.

| layer | holds | knows |
| --- | --- | --- |
| `source/` | one host interface: where it lives, its column order, its escaping | the model |
| `model/` | the types that render as a composed node | the value objects |
| `value_objects/` | the types that render as a leaf, a scalar or a list of scalars | nothing of the collector |

`source/` is an anti-corruption layer. `/proc/mounts` writes six positional
columns and escapes whitespace into octal; `/proc/modules` puts `[permanent]`
inside its dependants column and parenthesises taint letters; `dpkg-query` is
asked for tab-separated fields. None of that is what rastro means by a mount, a
module or a package, so none of it reaches the model. Adding
`/proc/self/mountinfo` later is a second source, not a change to `Mount`.

Two things make the split real rather than decorative. Each source names its own
record type (`ProcMountsLine`, `ProcModulesLine`) and maps it across, which is
also what caught a truncated-line bug that a slice pattern had been swallowing.
And `crates/rastro/tests/purity.rs` enforces the arrows by scanning source text,
including the layer aggregator files, so a model that reached back into a source
or into the execution seam fails the suite.

**Observations are produced by `From`**, not by an invented trait: it is the
language's own vocabulary for the conversion, and the orphan rule permits
`impl From<&Mount> for Observation` because the source type is local. A parser
therefore returns domain types and the tests assert on those rather than digging
through `Content::Object` maps. One test per collector pins the rendered key
names, because those are the output contract.

**Shared value objects live in `rastro-collector`**, beside `Presence`, which is
already one. That is the crate an outside in-process collector depends on under
the one-dependency promise, and a value it cannot reach is a value it will invent
its own spelling for, leaving two facets in one document disagreeing about what a
byte size looks like. `AbsolutePath` is built from `NonEmptyText` even though the
leading `/` already implies non-emptiness, so that no value object in the tree is
the exception that holds a bare primitive.

**Cost:** a collector is a dozen small files instead of one. The alternative was a
flat module per collector, which is what `mounts` originally was, and it put the
kernel's escaping rules inside the value objects.

## Keyed where the name is unique, listed where it is not

`modules` and `packages` render as an object keyed by name. `mounts` renders as a
list.

The rule is whether keying can lose anything. The kernel enforces unique module
names and a package manager enforces unique package names, so keying is lossless
and buys two things: ordering becomes structural through a `BTreeMap`, and
loading one module or installing one package shows up as a single added key. A
mount point is not unique, because stacked and bind mounts are real, so keying
would silently drop one of them and the kernel's own order is kept instead.

Where keying is used, a repeated name is an error rather than an overwrite. No
kernel and no package manager can produce one, so it means rastro misread the
output, and keeping the last of two would drop an entry from a document claiming
to be complete.

## A module's load address is not recorded

`/proc/modules` publishes each module's kernel text address. rastro drops it at
the source boundary: it never enters `KernelModule`, so no view can resurrect it.

Two reasons, and either would be enough. It changes on every boot, so it is pure
noise in a document whose worth is that two unchanged runs are byte-identical.
And it is a kernel pointer, so publishing it hands a KASLR offset to whoever
reads a fingerprint that has been copied off the box and committed to a
repository.

Marking it `volatile` was the obvious alternative and is wrong: the complete view
exists precisely to keep volatile values, so `--include-volatile` would print it.

## Shelling out goes through one hardened seam

Where parsing a canonical tool's output is more honest than reimplementing what
it does, a collector's source shells out through `collectors::canonical_tool` and
nothing else. rastro runs as root on production servers, so the seam guarantees,
each with a test:

- an absolute path resolved before exec, preferring well-known system paths over
  a `PATH` search, because a directory on root's `PATH` that is not root-owned
  would let a plant be executed with full privilege;
- no shell, an explicit argument vector, and no argument sourced from config or
  the command line;
- a cleared environment plus `LC_ALL=C`, which is hardening and determinism both,
  since a localised box would otherwise render different bytes for one state;
- immediate end of input, so a tool that prompts cannot wait for an absent
  operator;
- a time bound and an output bound, breaching either of which kills the tool's
  whole **process group**, so a helper it backgrounded does not outlive the
  failure;
- exit status checked, and stdout decoded as strict UTF-8 rather than lossily,
  because substituting `U+FFFD` would put text into a fingerprint that was never
  on the box.

The output bound needed care that is worth recording. `subprocess` enforces its
size limit by *stopping* the read and buffering the remainder, not by failing, so
taking it at face value would have returned a quietly truncated answer, which is
the exact configsnap defect that prompted this project. The limit is therefore set
one byte above the bound and anything past it is a recorded failure.

The seam cannot live in `rastro-collector`: that crate's `tests/purity.rs` forbids
`std::process`, which is correct, because an exec-contract author gets the port,
not the host.

**Crate-first, with one exception.** `subprocess` bounds the run and kills the group
through its own `JobExt::send_signal_group`, and `libc` supplies the `SIGKILL` constant
and nothing else. A `which` dependency resolved the path until the `PATH` search itself
was dropped, at which point it had nothing left to do. The exception is `/proc/modules`:
`procfs-core` parses it, but its `KernelModule` carries no taint field, so an
out-of-tree unsigned module would stop being visible. A twenty-five line parser that
keeps the state the tool exists to record beats a dependency that drops it.

A `nix` dependency was briefly added here to kill the group, on the false premise
that `subprocess` offered no way to signal one. It does, on `Job`, documented for
exactly the `setpgid` pairing rastro uses. Recorded rather than quietly reverted,
because the lesson generalises: a dependency justified by an absence in another
crate needs that absence checked in the crate's source, not inferred from the parts
of its API one happened to read.

## One packages collector, dispatching over the managers it finds

`packages` is one collector that reads every manager present, and its facet data
is keyed by manager.

One collector rather than one per manager, because two collectors claiming the
facet name `packages` would fail the run: an absent facet is still a facet with
that name. Keyed by manager rather than merged, because a box carrying two then
needs no arbitrary precedence, and the shapes may differ honestly, since only dpkg
reports a desired state.

**Every manager rastro reads is a key, and one that is not on the host is `null`.** The
facet is `ok` either way, and `presence` is always `Present`, because the subject is the
managers rastro can read and it can always report on those.

Neither of the other two answers is right, and both were tried. `Absent` claims the host
has no packages, which two negative probes cannot establish: a RHEL box has fifteen
hundred rpms, and rastro reads dpkg and apk. `Undetermined` maps to a facet `error`, and
rastro not shipping an rpm collector is a limit of rastro rather than a fault of the
host, so it would plant a permanent false alarm in every diff of that box. Slackware
makes the point from the other side: having no package manager is a legitimate state of a
host, not an error condition.

`null` rather than the word `absent` for one format reason: a key whose value is
sometimes text and sometimes an object is awkward for every consumer, and `null` is
already a leaf type the format admits. Installing a manager therefore shows up as `null`
becoming an object, which is the direction that matters in a diff.

**One failing manager costs the other's inventory**, and that follows from this shape rather
than being independent of it. `collect` propagates the first failure, so on a box carrying both,
a `dpkg-query` that times out makes the whole facet `error` and apk's packages go unreported.
The alternative is a per-manager error object in the data, which collides with the argument two
paragraphs down: a key whose value is sometimes an object of packages and sometimes an object
describing a failure is the shape every consumer has to special-case. Loud and whole beats
partial and ambiguous, and the port's `collect` is all-or-nothing by design. Recorded because it
is a consequence somebody will meet, not because it is in doubt.

There is no standard file naming a host's package manager, so nothing is inferred. The
closest marker is `ID` and `ID_LIKE` in `/etc/os-release`, which belongs in the `host`
facet as an observation; concluding "this box uses rpm" from it is the operator's
inference to draw, not rastro's to assert.

**dpkg is read through its tool, apk from its database**, and the inconsistency is
deliberate. `dpkg-query -f` makes the output format rastro's own, where
`/var/lib/dpkg/status` is a multi-line format dpkg's documentation says not to
parse. apk 3 offers no machine-readable output at all and every text form it
prints fuses name and version into one token, so using it would mean
reimplementing apk's name-version splitting grammar; `/lib/apk/db/installed` is
one field per line and unambiguous. The principle is not "always shell out", it is
"prefer the source that is unambiguous".

dpkg reports partially-installed packages and rastro keeps them: `config-files` for one
removed without purging, `half-configured` for one caught mid-operation. It does **not**
report every state, and the limit was measured rather than assumed: `dpkg-query -W` without
a pattern omits `not-installed` rows, so purging a package removes its key rather than
showing it as absent. Still diffable, and the alternative would be claiming a guarantee the
query does not give.

dpkg's status is asked for as three words (`${db:Status-Want}`,
`${db:Status-Status}`, `${db:Status-Eflag}`) rather than as the packed
`${db:Status-Abbrev}`, so dpkg decodes its own vocabulary, rastro maintains no
alphabet of status letters, and a diff reads `installed` rather than `ii`. A
package from apk carries no status rather than a fabricated one.

## No MSRV floor, the toolchain is pinned by mise

There is no `rust-version` in any manifest and no MSRV job in CI. `mise.toml` pins
the version the project builds with, and CI reads the same file.

A declared floor could never have been a support promise, because Rust ships every
six weeks and maintains only the latest release. Worse, it was not inert: this
workspace sets `resolver = "3"`, and cargo reports `Locking N packages to latest
Rust <floor> compatible versions`, so a floor holds dependencies back to suit a
compiler nobody runs. Nothing was being held back when it was removed, but the
first dependency to raise its own floor would have been silently pinned.

**Cost:** a contributor on a distribution's own rustc may need mise or rustup. That
is already true of anyone building a static musl binary.

Reintroduce a floor per crate, deliberately, if `rastro-collector` is ever
published, since a third-party collector author is the only audience a floor has.

## A facet's error text is not classified, yet

A failing collector's message, including a bounded tail of a tool's stderr,
reaches the document's `error` field without passing the `sensitive` and
`volatile` classification that every observed value goes through. `CollectionError`
is a bare string and the serialiser writes it verbatim.

This is recorded as a known exception rather than designed around, because the
mechanism it would participate in does not exist: the `sensitive` annotation is
carried and nothing acts on it, and `--raw` is not built. Widening the port's
error type now would be guessing at a shape that cannot be tested, inside a
contract a third-party collector compiles against. The content at stake today is
paths and hostnames, which rastro already publishes deliberately.

**Revisit when redaction lands.** Deciding whether diagnostic text is an observed
value is a prerequisite of that work, not an afterthought to it.

## A fingerprint is sensitive operational data until redaction exists

The document is not merely a description of a host, it is a target-selection aid.
The `packages` facet emits a complete name-and-exact-version inventory, which turns
CVE lookup into a filter, and `modules` names every loaded driver including
out-of-tree and unsigned ones. Nothing marks any of it `sensitive`, and nothing
would act on the annotation if it did.

The existing stance, that redaction is "an option, not a guarantee" and that marking
fields is the collector author's job, was written when the only collectors reported a
hostname and a mount table. Package and module inventories are a different order of
exposure, so the stance is unchanged but its consequence is now stated plainly:
**a stored fingerprint should be handled as sensitive operational data**, not
committed to a repository that is more widely readable than the box it describes.

Whether `PackageVersion` and the module taint flags should carry the `sensitive`
annotation is deferred to the same point as the previous entry, because an
annotation nothing acts on would be decoration.

## Accepted residual risk: signalling by pid

`canonical_tool` calls `poll` before signalling a tool's process group, which reaps
an already-exited child so the crate's own guard can skip the signal. Between that
check and the signal there remains a window in which the pid could be reaped and
recycled by an unrelated process that is itself a group leader, which would then
receive a stray `SIGKILL`.

This is inherent to signalling by pid on Unix rather than anything rastro
introduces, and closing it properly needs `pidfd`. Accepted, and recorded here so it
is a known residual rather than an oversight.

## Superseded: the group signal is unconditional

Supersedes [Accepted residual risk: signalling by pid](#accepted-residual-risk-signalling-by-pid),
which described a `poll` call that no longer exists.

That entry accepted a window between polling the job and signalling its group, on the grounds
that a reaped pid could be recycled. A challenge round then showed the guard was not merely
imperfect, it was actively hiding the bug it sat next to: a tool that backgrounds a helper and
exits at once leaves the helper holding the pipes open, `poll` reports the direct child gone, and
the early return spared exactly the descendant the group kill exists to reach. The test that
covered the area used a parent that was still alive at kill time, so it exercised the branch that
worked and never the one that did not.

The signal is now sent unconditionally, and the window the old entry accepted does not arise. A
group with living members cannot have its leader's pid recycled, because each member holds that
`struct pid` as its group id, and a group with nothing left in it simply yields `ESRCH`. This was
checked against `subprocess`'s own `WNOWAIT` rationale rather than assumed.

**What remains, and it is a different shape.** `send_signal_group` no-ops once the crate has
cached an exit status, so the unconditional signal only helps because nothing calls `wait` or
`poll` before it. That dependency is real and nothing enforces it, so it is named in the code at
the point where it would be broken, and
`run_kills_a_descendant_of_a_tool_that_already_exited` is the test that would catch it.

**Cost:** a residual risk that was recorded as accepted turns out to have been the wrong trade,
and the entry recording it stood for four commits while describing code that had been deleted.
The lesson is the one the `nix` reversal already taught: an entry justified by a mechanism needs
re-reading whenever that mechanism changes, and neither of the two commits that touched this file
afterwards did so.

## The accounts collector records no password hash, so it cannot see a password change

`/etc/shadow` is read, and the hash column is classified and dropped where the line
is parsed. What reaches the document is a state (`absent`, `unusable`, `locked`,
`usable`), the placeholder a tool wrote when there is no hash, and the crypt
algorithm identifier when there is one. No type in the collector has a field a hash
could be stored in.

**Why not carry it and mark it sensitive.** That is what `Sensitivity::Sensitive` is
for, and it would be the right answer if the redaction layer existed. It does not:
the annotation is recorded and nothing acts on it yet, so a hash marked sensitive
today is a hash printed to stdout in plain text. The box this was developed against
has a live yescrypt hash in `/etc/shadow`, so this is not hypothetical. Absence of a
field is a guarantee the unbuilt redaction layer cannot weaken; an annotation is a
promise about code that is not written.

**The cost, stated plainly, because it is the reason this entry exists.** The hash is
the only part of a password that changes when the password changes. So changing a
password does not change this facet: the state stays `usable`, the algorithm stays
the same, and a diff either side of `passwd` is empty. Anybody reading an `accounts`
diff as evidence that authentication was untouched is reading it wrong.

What is still visible: an account arriving or leaving, a uid, home or shell changing,
a password appearing or being removed at all, an account being locked or unlocked,
the hashing scheme changing under a release upgrade, and group membership changing —
which on a key-authenticated box is how privilege is actually granted.

**One field narrows the gap.** `shadow(5)` defines column three as the date of the
last password change and `passwd` rewrites it when it writes a hash, so
`last_changed_days_since_epoch` moves, to a resolution of one day. That is the file's
documented contract rather than something rastro measured, and it is defeated by a
tool that edits the hash column directly. Locking does not move it, because
`usermod -L` only prefixes the hash.

**Reversing this** means a redaction layer that hashes a value before it is rendered,
at which point a digest of the hash becomes recordable and a password change becomes
visible without the credential ever being printed. That is a new entry, not an edit
to this one.

## Collectors ask their tool for JSON, which promotes serde_json to a real dependency

`serde_json` was a dev-dependency, used only by tests reading the rendered document
back. The units collector makes it a normal dependency of `rastro`.

**Why.** `systemctl` prints a whitespace-aligned table with a trailing free-text
description. Splitting it means guessing where a column ends, and the guess is worst
exactly where the data is most awkward: device unit names run to hundreds of
characters and carry systemd's `\x2d` escaping, and the alignment shifts with them.
`systemctl --output=json` removes the guess, and systemd 252, which Debian 12 ships,
supports it on both subcommands this collector uses.

This is the same reasoning that already has the packages collector query dpkg through
`dpkg-query -f` rather than reading `/var/lib/dpkg/status`: **prefer the source whose
shape rastro chooses over the one it has to infer.** The principle was already written
down; this entry only records that honouring it costs a dependency.

`ip -j` and `lsblk -J` offer the same, so the network and block-device collectors
inherit the decision rather than each re-arguing it.

**Cost, and why it is small.** One more crate linked into a binary that runs as root:
that is exactly what `deny.toml` exists to police, and `serde_json` passes it
unchanged, being MIT/Apache-2.0 and already in the lockfile at the version the tests
were using. It is pure Rust with no build script and no C, so the static musl build is
unaffected. `serde` itself was already a normal dependency for the config layer, so no
new derive machinery arrives.

**What was rejected.** Parsing the tables and keeping the dependency out. It is
strictly more code, and the code is the fragile kind: a column-guessing parser that
passes on the fixtures the author thought of and mis-slots a unit name nobody
anticipated. Refusing a dependency at the price of a parser that can be quietly wrong
is the wrong trade for a tool whose one unacceptable failure is reporting something it
half-understood as complete.

## Superseded: the time collector reads files, because `timedatectl` starts a unit

The time collector was written to run `timedatectl show`, on the rule that effective,
resolved state beats reading configuration files. That was the wrong call, and the
reversal was forced by CI rather than reasoned out in advance.

**`timedatectl` starts a systemd unit on the box being fingerprinted.**
`systemd-timedated.service` is `Type=dbus`, so the first D-Bus call activates it and it
keeps running afterwards. Measured, not inferred: with the unit stopped,
`systemctl list-unit-files` left it `inactive`, and a single `timedatectl show` left it
`active`.

**How it surfaced.** The determinism harness failed in CI and nowhere else. The unit that
the `time` collector started appeared in the *next* run's `processes` facet, so two runs of
an unchanged host differed by exactly one process. It could not reproduce locally: macOS
has no `/proc`, and an idle container has neither systemd nor the tools. It reproduced on
the Debian test box only under load, which is what CI is — every tool present, two runs
seconds apart.

**Two reasons to reverse it, either sufficient.** A fingerprint must not change the box:
rastro runs as root on production to observe, and starting a unit is a mutation however
small. Nothing else it runs does this — `systemctl`, `ss`, `ip`, `lsblk`, `iptables-save`,
`dpkg-query` and `sshd -T` all leave the box as they found it. And the byte-identical
diffable view is the contract everything else rests on, so a collector that breaks it is
wrong whatever else it gets right.

**What the files give.** `/etc/localtime`'s symlink target for the zone, with
`/etc/timezone` as the fallback and the symlink winning when they disagree, because the
symlink is what programs follow. `/etc/adjtime`'s third line for the hardware clock's
scale, where an absent file means UTC — its documented contract, and the case on the test
box. `/run/systemd/timesync/synchronized` for whether synchronisation has happened.

**What it gives up, and why that is acceptable.** `CanNTP` and `NTP`: whether a
time-synchronisation service exists and whether it is switched on. Neither leaves the
document, because both are the enablement state of a unit and that is the `units` facet's
answer — `systemd-timesyncd.service` appears there as `enabled`. One fact in one place is
better than the same fact in two.

**The collector's version went to `2`**, because the facet lost two fields. A consumer
diffing across the change has to be able to see that the collector moved rather than the
host.

**The general rule this does not overturn.** Prefer effective state over configuration
files still holds; `sysctl`, `systemctl` and `sshd -T` are all still read that way. What
this adds is a precondition: prefer the effective source *unless reading it changes the
host*. `nginx -T` and `sshd -T` do not. `timedatectl` does.

## The determinism harness names the facet that differed

The harness compared two runs as `Vec<u8>` and asserted equality. When it finally caught
something, it reported two four-hundred-kilobyte byte arrays into a CI log, which is worth
nothing to whoever reads it — the failure above took a reproduction on a real box to
diagnose, and the test had the answer all along.

The comparison is still on bytes, because bytes are the contract. On failure it now parses
both documents and names each facet that differs, with a bounded excerpt of each side.
That is a diagnostic on the failure path only, so it is free to be slower than the
assertion it explains.

# Telemetry: what runs on the box, and what watches it

Dated 2026-08-24. Driven by a box running six telemetry agents, none of which
the fingerprint could see.

## A unit records what it starts, resolved by systemd

The `units` facet reported enablement and runtime state and never said which
binary a unit amounts to. "This service is enabled and active" is a weaker claim
than an operator assumes: the unit file can be rewritten to start a different
program, with different flags, and every field the facet carried would be
unchanged.

It now carries each unit's effective `ExecStart=`, asked of `systemctl show`
rather than read from the unit file, so drop-ins under `<unit>.d/` are already
applied. That is the same preference for effective state that has `sshd -T`
asked instead of `sshd_config` parsed.

**Asked of systemd rather than read from `/proc`.** The process table carries the
same argument vector for a *running* service, and the processes facet already
records it. systemd answers for a unit that is enabled and dead, which is a
configuration that exists and has no process behind it. Where the two disagree —
a unit edited without a restart — both facets are in the document and the
divergence is visible.

**Cost, accepted:** a second `systemctl` call per run, over every loaded unit.

**The glob is a trap, and it is silent.** `systemctl show '*.service'` answers
for 47 of the 109 service units `list-units --all` reports on the development
box, with no error and no warning. Every unit is named explicitly instead, after
a `--`, because systemd's own root slice and root mount are called `-.slice` and
`-.mount` and `systemctl` otherwise rejects them as invalid options.

## A unit's argument vector is recorded whole, not split

Measured, not assumed: a unit reading `ExecStart=/bin/echo --flag="a b" second`
comes back from `systemctl show` as `argv[]=/bin/echo --flag=a b second`.
**systemd does not preserve the quoting**, so three whitespace-separated tokens
stand for two arguments and nothing in the output says which.

Splitting there would claim a structure the source cannot support, and would be
silently wrong for exactly the units whose arguments are interesting. The vector
is kept as one string.

**The exporters facet does split, and the difference is what makes it safe.**
Every agent it knows takes `--flag=value`, so a token that is not a flag can only
mean the vector was not what rastro assumed — a recorded failure naming the
argument. A bad split is refutable there and is not refutable in the general
case.

## Layer 3: a telemetry fleet facet, dispatched from the binary a unit starts

Six telemetry agents run on the development box and **`dpkg` has heard of exactly
one of them**. collectd is a Debian package at `5.12.0-14`; cAdvisor,
node_exporter, process-exporter, systemd_exporter and postgres_exporter are
binaries dropped into `/usr/local/bin` by Ansible, invisible to every package
manager on the host. Their versions exist nowhere on the box except inside the
binaries, so the facet runs them and asks.

This is the fourth Layer 3 collector, alongside the nginx, postgres and docker
starters, and it is the first where the dispatch signal is the **binary a unit
starts** rather than the unit's name. `process_exporter.service` runs a program
called `process-exporter`, underscore against hyphen, and an operator may name a
unit anything at all.

**Keyed by unit, not by agent.** A box with two PostgreSQL clusters runs two
`postgres_exporter` instances on two ports. Keying by agent would let the second
silently overwrite the first; systemd enforces one unit per name.

## The exporters facet knows its agents by name rather than by heuristic

A fixed catalogue of six agents, each with the dialect it uses to report a
version and the dialect it uses to spell its listen address.

The alternative is worse rather than more general. A heuristic — "any unit with a
`--web.listen-address`" — would sweep in unrelated daemons that happen to use the
flag, and would still miss cAdvisor and collectd, neither of which uses it.
cAdvisor takes `--listen_ip` and `--port` separately; collectd takes no arguments
at all and gets its port from a plugin in `/etc/collectd/collectd.conf`.

**Cost, accepted knowingly:** an agent not in the catalogue is not in the facet.
That is a visible gap rather than a half-read entry, and the units facet still
records what its unit starts.

**No defaults are filled in.** Only flags the unit actually passes are recorded.
Every one of these agents compiles in a default for each flag it was not given,
and writing those into the facet would mean shipping a copy of somebody else's
flag table and asserting values rastro never observed.

## The execution seam can capture stderr

`CanonicalTool::run` returns stdout, which is what almost every collector wants.
Measured on the development box: `node_exporter --version` and
`process-exporter --version` print to stdout, while `systemd_exporter --version`
and `postgres_exporter --version` print the same text to **stderr**, all four
exiting zero.

A collector reading stdout alone would report half the fleet as having no version
— wrong, and quietly so, rather than a recorded failure. `run_capturing_stderr`
returns both streams. Every other guarantee is unchanged: same bounds, same group
kill, same refusal of a non-zero exit and of invalid UTF-8.

Reading stderr on success is not the same as trusting it on failure: a failing
tool's stderr is still only quoted back as a diagnostic.

## A configured endpoint is a different fact from a bound socket

The `exporters` facet records the address an agent's **flags asked for**. Whether
anything is listening there is the `sockets` facet's answer, read from the
kernel. The two are deliberately separate observations, and the point is that
they can disagree: an agent configured for 9100 with nothing bound to it is a
dead exporter, and only two independent observations can show that.

Verified on the box: all five flag-configured agents are bound where they were
configured, and collectd's 9103 appears as bound-but-not-configured — exactly
right, because that port comes from a plugin config and rastro declines to invent
it from a flag that is not there.

**An agent's own measurements are not here and never will be.** Container CPU and
memory numbers change by the second; a fingerprint records what a box *is*, not
what it is doing.

# Layer 1: what the walk records about one entry

Dated 2026-08-27. Driven by the walker itself existing: the entries below are the
attribute-depth questions it could not be written without answering.

## An inode's timestamps are nanoseconds since the epoch, and atime is not recorded

`st_mtim` and `st_ctim` are a second and a nanosecond each, and both halves are
kept, combined into one integer. Rounding to the second would make two writes
inside one second the same fact, the format admits no floating point so a second
with a fraction is not on offer, and rendering a calendar date would mean rastro
being right about every zone and leap-second rule to gain readability and no
signal. It is the same reasoning that keeps a systemd timer's moment in
microseconds, and the units collector's precedent for the shape.

**Both stamps, not one.** They are separate facts, and the pair is what makes a
tampered one visible: a `chmod` moves the ctime alone, and a `touch -d` that
backdates the mtime cannot move the ctime backwards with it.

**Neither is volatile.** A file nobody touched carries the same stamp on both
runs, so the byte-identical guarantee holds with these in the document. Stamp and
lock files whose only churn *is* their mtime are noise in a diff — 23 of the 112
files that changed on the reference box in seven days are exactly that — but they
are real changes, and the answer to them is the walker's exclusion scope rather
than pretending the value moves on its own.

**There is no atime.** rastro reads a file's content to hash it, which moves that
file's access time, so recording atime would report the tool's own visit as a
change to the box. The one attribute a fingerprint must not carry is the one the
fingerprinting created.

**Cost:** every entry carries two more integers, and a tree whose files are
rewritten identically now diffs where before it did not.

## A device node records its major and minor numbers

`st_rdev` is split into the two numbers `major(3)` and `minor(3)` yield, and
carried for block and character devices only. A device node has no content to
hash and no size worth recording: the numbers are the whole of its state, and
`/dev/sda` becoming `8:16` where it was `8:0` is a different disk under the same
path. Recording the kind alone would let two inventories addressing different
devices compare equal.

Split here rather than left packed, because the packed form is a Linux encoding
that scatters both numbers through a 64-bit word, and a reader should not need to
know it to read the document.

**Cost:** the split is the kernel's, so a port to a system that packs `st_rdev`
differently changes this one function.

## Hashing opens with `O_NOFOLLOW` and `O_NONBLOCK`

The `symlink_metadata` that classifies an entry as a regular file and the open
that hashes it are two calls, and on a live box a package upgrade lands between
them. A pathname open would then follow a replacement symlink out of the walked
tree, or block forever on a replacement fifo — as root, on production, with the
never-follow promise the walker makes broken and nothing in the output saying so.

The flags refuse both, and the file type is checked again on the descriptor
rather than on the path, because the descriptor is the thing actually being read.
A mismatch is a recorded failure, not a digest of whatever arrived.

**What this does not fix:** a regular file replaced by another regular file
between the two calls still hashes the replacement. That race has no fix at this
layer — the entry describes what was at the path when the walk reached it — and
the inode is recorded, which is what makes the swap legible afterwards.

## ACLs and extended attributes are owed, not dropped

`design.md` lists POSIX ACLs and xattrs per entry, and the walk records neither
yet. On a host where access is decided by an ACL, an SELinux label or
`security.capability`, a change to any of them leaves every field the walk does
record identical, so the gap is real and it is a gap in what the tool claims.

It is deferred rather than half-built because "which attributes" is one decision,
not three: ACLs are themselves stored as xattrs, the interesting security ones
are namespaced differently, and enumerating every attribute on every file has a
cost the walk has not measured. The walk has no `*xattr(2)` seam at all today, and
adding one for a third of the answer would fix the shape before the question is
settled.

**Cost:** until it lands, a capability-only or label-only change is invisible to
rastro, and that is a known false negative rather than an unknown one.
# PostgreSQL: the server's effective and observed state

_2026-08._ The facet grew from reading a cluster's settings to reading what the
server is actually running with, and these are the decisions that shaped it.

## pg_settings is one session's view, not the cluster's

`pg_settings` is a projection of the connecting backend's own GUC array, not a
cluster-wide catalogue. It folds the reading role's and database's `ALTER ROLE` /
`ALTER DATABASE` defaults into its map as though they were global, and it silently
drops the 21 `GUC_SUPERUSER_ONLY` rows for a role that is neither a superuser nor a
member of `pg_read_all_settings`. Read alone, it is confidently wrong in ways a
diff cannot see.

So the facet does not trust it alone. It records the **lens** the settings were
read through (role, database, superuser, `pg_read_all_settings`) and derives
**`settings_complete`**, which goes false when that lens dropped the privileged
rows. It reads **`pg_db_role_setting`** apart, so the scoped defaults are visible
as scoped rather than folded into the map. And it reads **`pg_file_settings`**,
which re-parses the files, so a value edited without a reload, and a line that will
not apply at all, are both seen.

**Cost:** several more reads per cluster, all server-wide from the one connection
the facet already opens. The credential-bearing settings are redacted by name, so
the added reads do not widen what leaks.

## A cluster's running port comes from postmaster.pid, its configured port from pg_lsclusters

The same rule the `exporters` and `sockets` facets already follow: a configured
fact and an observed fact are kept apart so they can disagree. `pg_lsclusters`
prints the port from `postgresql.conf`, which is stale the moment the file is
edited without a reload; `postmaster.pid` line 4 is the port the server is actually
serving on. The facet records both, and connects on the observed one, so a
stale-config port can no longer make a live cluster read as `down`. `postmaster.pid`
also carries `PM_STATUS`, which tells a standby deliberately refusing connections
apart from a broken cluster.

**Cost:** a privileged file read at the data directory pg_lsclusters names. Absent
is not a failure: a cleanly stopped cluster has removed the file.

## Only the stable columns of a moving catalogue are read

`pg_control` and `pg_replication_slots` each carry a mix of state and motion. The
facet takes the stable half and leaves the rest: from `pg_control`, the system
identifier and the timeline (which say which cluster this is and whether it was
promoted), never the LSNs, xids and checkpoint time that move on every checkpoint;
from a replication slot, its identity and shape, never its `restart_lsn`,
`confirmed_flush_lsn`, `wal_status` or `active` flag. `pg_hba_file_rules` is read
version-aware, because `rule_number` and `file_name` are PostgreSQL 16 additions
and asking for them on 15 would fail the read.

**Cost:** the moving columns are genuinely useful to an operator watching a slot
catch up, and a fingerprint deliberately does not carry them. That is a job for a
monitor, not a diff.
