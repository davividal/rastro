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

**Open:** the chosen view is not recorded in the document, so diffing a complete
against a diffable one produces nonsense with no warning. It resolves itself
when the config layer lands: the view is a CLI flag, flags are part of the
effective config, and the `invocation` facet already promises to carry that.

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

It was also internally inconsistent with this file's own "exclusions, never
inclusions", which presupposes a default scope to exclude *from*.

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
