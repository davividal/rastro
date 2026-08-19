# Decision log

Decisions that are settled. Each entry states the choice, the reasoning, and
what it costs. Reversing one is a new entry, not an edit to an old one.

All entries below date from the initial design, 2026-08-13.

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

**Crate-first, with one exception.** `which` resolves the path, `subprocess` bounds
the run and kills the group through its own `JobExt::send_signal_group`, and `libc`
supplies the `SIGKILL` constant and nothing else. The exception is `/proc/modules`:
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
