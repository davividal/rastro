# Decision log

Decisions that are settled. Each entry states the choice, the reasoning, and
what it costs. Reversing one is a new entry, not an edit to an old one.

Entries are grouped by the work that produced them, and each group is dated where it begins.

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

## A Cargo workspace replaces the hexagonal layout

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

| assertion | now |
| --- | --- |
| domain depends on no other layer | refused by cargo; a crate cycle does not compile |
| application names no adapter | refused by cargo, same reason |
| domain reads nothing from the host | ported, one `tests/purity.rs` per library crate |
| domain modules form no cycle | ported into `rastro-fingerprint/tests/purity.rs` |
| the module graph matches a record | retired; the cycle walk carries the part that mattered |
| observations know nothing about documents | covered by that cycle walk, with the direction recorded in a comment |
| presentation and infrastructure do not know each other | **not** a crate boundary: `cli` and `collectors` are siblings in `rastro`, so it kept its own test |

It also settles the cycle argument. The
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

**Modules are named for the model, not for the types they hold.** Inside
`rastro-fingerprint`, the modules are the model's joints: `fingerprint` (the
document and its consistency rules), `observation` (what was seen and what the
seer asserted about it), `collector` (who observes, under what contract). A
`FacetName` lives beside `Facet` because it names one, not in a file called
`identifier` beside every other newtype. Grouping by what a type *is* to the
compiler rather than by what it *means* leaves the module structure carrying no
information about the model.

**Deliberately not adopted: repositories and unit of work.** rastro persists
nothing, has no database, no transaction, and no aggregate to load by id. The
nearest thing is the output sink, which is a one-way write, not a repository.
Those blocks are load-bearing in the backend templates the layering was borrowed
from and would be empty ceremony here.

## Config is optional, opt-in and exclusion-only

Replaces the earlier rule that a config file was mandatory and looked for beside
the binary.

`rastro` with no arguments collects everything. `--config <path>` can only
narrow that, and there is no way to say which collectors *do* run.

**The old rule contradicted the project's founding disqualifier.**
`docs/research.md` rules out any tool that "requires you to declare *what to
watch*", because if you could enumerate what changes you would not need the
diff. That is what disqualified AIDE and configsnap. Refusing to run without a
config file is the same disqualifier one level up.

It was also internally inconsistent with the project's own "exclusions, never
inclusions" (`CLAUDE.md`), which presupposes a default scope to exclude *from*.

**What the old rule was reaching for was never explicit input.** It is that a
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

**Both have landed** — see
[`--raw`, and a document that admits which one it is](#--raw-and-a-document-that-admits-which-one-it-is).
The decision above stands; the reason it gave for deferring does not.

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

## The group signal is unconditional

`canonical_tool` used to `poll` the job before signalling its process group, skipping the signal
when the direct child had already been reaped. That guard was accepted as leaving a pid-reuse
window. It was worse than imperfect: it was hiding the bug it sat next to: a tool that backgrounds a helper and
exits at once leaves the helper holding the pipes open, `poll` reports the direct child gone, and
the early return spared exactly the descendant the group kill exists to reach. The test that
covered the area used a parent that was still alive at kill time, so it exercised the branch that
worked and never the one that did not.

The signal is now sent unconditionally, and the window the guard was accepted for does not
arise. A
group with living members cannot have its leader's pid recycled, because each member holds that
`struct pid` as its group id, and a group with nothing left in it simply yields `ESRCH`. This was
checked against `subprocess`'s own `WNOWAIT` rationale rather than assumed.

**What remains, and it is a different shape.** `send_signal_group` no-ops once the crate has
cached an exit status, so the unconditional signal only helps because nothing calls `wait` or
`poll` before it. That dependency is real and nothing enforces it, so it is named in the code at
the point where it would be broken, and
`run_kills_a_descendant_of_a_tool_that_already_exited` is the test that would catch it.

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

## The time collector reads files, because `timedatectl` starts a unit

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

**Verified on PostgreSQL 17.11:** a superuser saw 380 settings and a non-superuser
role 358, silently, with no error either side; `SHOW data_directory` as that role
raised `permission denied to examine "data_directory"` (42501), and its lens read
`is_superuser=f, pg_read_all_settings=f`, so `settings_complete` is false. Querying
`pg_file_settings` in one session flipped `max_connections`'s `pending_restart`
from false to true, while a fresh session read it false again, which is why each
catalogue is read on its own connection.

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

**Verified on Debian 12 / PostgreSQL 15:** with the port edited to 5433 in
`postgresql.conf` and not reloaded, `pg_lsclusters` reported 5433 while
`postmaster.pid` line 4 and the running server stayed on 5432. The pid file's line
8 was `ready` (space-padded), lines 1 and 3 the volatile PID and start time.

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

**Verified on a box:** `pg_control_system()` returned the system identifier for a
non-superuser role on PostgreSQL 17, confirming the `pg_control_*` family is
EXECUTE-to-PUBLIC. On Debian 12 / PostgreSQL 15, `pg_hba_file_rules` had nine
columns (no `rule_number`, no `file_name`), and hiding the server binary made
`pg_lsclusters` print `down,binaries_missing`, the qualifier the status parser now
records rather than fails on.

**Cost:** the moving columns are genuinely useful to an operator watching a slot
catch up, and a fingerprint deliberately does not carry them. That is a job for a
monitor, not a diff.

# Layer 1: the noise a real before-and-after produced

Dated 2026-08-28. Driven by the first full cycle against the reference box, where
applying an Ansible role added 22 packages: 568 entries added, 2 removed, 163
modified, and a two-run noise floor of six paths measured before any of it was
attributed to the change.

## A directory's stamps and link count are derived, so they are volatile

Refines [Neither is volatile](#an-inodes-timestamps-are-nanoseconds-since-the-epoch-and-atime-is-not-recorded),
which said the answer to stamp churn is the walker's exclusion scope. That holds for
a file, and it does not hold for a directory, because a directory's `st_mtim`,
`st_ctim` and `st_nlink` are not observations of the directory at all: they are a
summary of the entries under it, and the walk reports every one of those entries in
its own right. So the value moves on its own as far as the reader is concerned, and
it says nothing the neighbouring keys do not already say.

`FileKind::summarises_what_is_inside_it` is the whole rule, and it is `true` for a
directory only. A regular file keeps both stamps and its link count in the diffable
view: nothing derives them, an in-place rewrite that kept the size moves the mtime
and only the mtime, and the link count is how a hardlink shows at all.

**Measured, not assumed.** Of the 163 modified entries in the reference cycle, 104
were directories whose only change was these three fields, and 64 of those had an
added or removed child in the same document. The remaining 40 are apt, dpkg and man
cache directories, where the fact that something churned inside is exactly what the
entries inside report.

**Volatile, not dropped.** The stamps are still read and still rendered, so
`--include-volatile` answers the operator who does want to know when a directory
moved. Derived is not unobserved.

**Cost:** a directory whose child was created and deleted between two runs now shows
nothing in the diffable view, where before it showed a stamp. That is the intended
trade: the diffable view carries what a reader can act on, and an event with no
surviving trace is not that.

## A collector claims the trees it owns, and the walk narrows to fit

The filesystem walk is agnostic by design and that is its whole value: it reads every
mount that holds files and needs no declaration to find anything. It is also why it
cannot know that `/var/lib/postgresql/17/main` is a cluster whose catalogues the
`postgresql` facet already reports properly, or that reading it on a real database
server means hashing a petabyte.

The collector that owns the tree knows both. `Collector::filesystem_claims` is how it
says so, and the vocabulary is three steps back from the default: `MetadataOnly` (stat
everything, open nothing), `Churns` (and the attributes that move on their own are
volatile), `Sealed` (record the tree's own directory and do not descend).

**A claim only narrows, and the type is what enforces it.** `ClaimedReading` cannot
spell "hash this", so no claim can widen the walk, ask for an algorithm, or reach a
tree the operator excluded. The config layer follows the same rule by policy; here it
is unspellable.

**Through the port, so nobody depends on anybody.** `WalkedTree` and the claim types
live in `rastro-collector`, which both sides already depend on. The walk consumes
claims without knowing who wrote them, a claimant names a tree without knowing
anything about walking, and `collectors.rs` is the only place that knows both, because
registration is already its job.

**Resolved from the host, not declared.** A claim names the path the collector found,
not the one its distribution's default would use, for the same reason the facet reads
`pg_lsclusters` rather than assuming a data directory. A claim that cannot be resolved
is left unmade: the walk's own default is the safe direction to be wrong in, and it is
loud rather than silent.

**Asked of every built-in collector, including one the config excludes.** The narrower
of two wrong answers. Releasing a claim because its facet was excluded would make an
exclusion *widen* the walk, so `--exclude postgresql` would quietly put a cluster's
data directory back under the hashing default and hash 300 MB of WAL on the way past.

**Cost:** a claimant's mistake is now a fingerprint's blind spot, and it is a mistake
made in a different file from the one whose output changes. The effective table is what
makes it visible, which is why the next entry is not optional.

## The effective walk table travels in the `invocation` facet

Three doc comments already promised this and nothing implemented it: the `invocation`
facet carried `excluded_collectors`, `source` and `view`, and no reader could tell a
missing digest from a policy decision. With collectors able to change that policy, the
promise became load-bearing.

The table renders keyed by tree, each rule carrying its `reading` and the facet that
asked, `claimed_by`. rastro's own shipped rules name the `filesystem` facet, so every
rule has a claimant and there is no absent case to interpret.

**In `invocation` rather than in `filesystem`.** It is a decision this run made, not
state observed on the host, and that is exactly what the `invocation` facet is for. It
also keeps the largest facet in the document from carrying its own legend.

**Cost:** one more object in the envelope, and a diff of two hosts with different
claimants now differs there as well as in the entries. That is the point: a table that
moved is a change worth seeing.

## A tree two collectors claim fails the `filesystem` facet

Two rules for one tree leave no most specific answer, and every way of picking a winner
would be rastro deciding for the operator which of two collectors was right about a
tree neither should have been arguing over. It is a bug in a collector pair, and the
box that produces it is real: a MySQL and a MariaDB collector both naming the same data
directory because neither resolved it from the host.

So the fold fails, and the message names the tree and both claimants. A claim that
merely repeats a shipped rule is a conflict too, because agreeing by accident is not
agreement, and the next release moving either side would turn a silent duplicate into a
silent disagreement.

**The facet, not the run.** The conflict makes the walk unanswerable and leaves every
other facet as true as it was, so `FilesystemCollector` holds the unresolved table and
reports the conflict as its own `error`. Failing the run would cost an operator the
whole document over a bug in two collectors they did not write.

**Cost:** the largest facet in the document can be lost to a mistake in an unrelated
collector, and the walk is where it surfaces rather than where it was made. The message
carries both names for exactly that reason.

## A tree that churns without meaning stops reporting the attributes that move

`CHURNS_WITHOUT_MEANING` was `MetadataOnly`, which withheld the digest and nothing
else. Measured on the reference cycle, that left the very noise the list exists to
remove: both journals and the timesync clock still in the diff on mtime alone, and
`/var/cache` on size and inode. So the shipped list is `Churns`, and size, inode and
both stamps are volatile under it.

What survives is presence, kind, permissions and ownership, which is what an operator
can act on in a tree that writes to itself. `Sealed` churns too, since the only entry
it produces is a directory whose stamps move for contents nothing is going to report.

**Measured:** with the derived-stamp rule and this one together, the 163 modified
entries of the reference cycle become 17, and with the three claims that follow, one:
`/etc/ld.so.cache`, which genuinely changed because 22 packages landed.

**Cost:** a log file rewritten to a different size no longer shows in the diffable
view, and neither does a journal replaced wholesale. The complete view still carries
both, and `/var/log` was never the tree a fingerprint was watching.

## Only a staged run omits the binary, and the caller says so

The walk used to omit the executable it was started from, unconditionally. The omission was
right; making it unconditional was not.

**The question that broke it:** can rastro recognise itself wherever it is? It already
does, exactly, and that was never the problem. `/proc/self/exe` is a kernel link to the
running inode, and it identifies the file better than any alternative: `argv[0]` is
caller-controlled through `execve`, is often a bare name rather than a path, and a
supervisor may rewrite it. Hashing itself at runtime would work too, and would make
things worse: the staged copy and an installed `/usr/local/bin/rastro` are byte-identical,
so recognition by content would hide every copy of rastro on the box rather than the one
that is running.

**What rastro genuinely cannot tell from inside one run is whether the file is
transient.** A `mktemp` copy that `rastro-ssh`'s trap deletes is not host state. An
installed binary is, and a swapped one is exactly the change this tool exists to catch.
Identical bytes, identical kernel link, different facts, and the only party that knows
which is which is the one that made the copy.

So the knowledge travels with the invocation: `--staged` says "this executable is a
temporary copy", `rastro-ssh` passes it because it did the staging, and only then is the
path left out. A local or installed run reports its own binary like any other file.

**The omission stays accounted for, and now honestly.** `staged_binary` is in the
effective config, unannotated, so the *diffable* view says the omission was requested;
`observer` still carries the path, volatile, because a `mktemp` name really does change
between two runs of an unchanged host. The previous version annotated an installed
binary's stable path as volatile, which was a lie by the format's own definition of the
word.

**Verified on the reference box:** with `rastro-ssh`, `staged_binary` is `true`, no
`/var/tmp/rastro.*` entry appears, and two runs 15 seconds apart are byte-identical
across the whole document. Run directly without the flag, the same binary reports its own
path with mode and owner, and `staged_binary` is `false`.

**Rejected: a deterministic staging path**, which would need no omission at all because
the entry would be identical in both runs. `/var/tmp` is world-writable and sticky, so a
fixed name is a symlink target for any local user, which is the reason `mktemp` is there.
A fixed name under `/root` is safe from that but puts the tool's footprint in a hashed
tree on every run and breaks for a non-root operator.

**Cost:** a flag that a caller must remember, and a wrapper is the only caller that
should. Forgetting it costs one entry of noise per run; passing it wrongly on an
installed binary hides one file, and the effective config says so in the default view.

# Distribution: getting the binary before there is a release

Dated 2026-08-30. rastro is at 0.0.0 and the tag-triggered release job is still
owed. This group covers only the gap: how somebody gets the newest build in the
meantime.

## The newest master build is a moving pre-release, not a release

CI has always built the musl binary and uploaded it as a run artifact. That is not
nothing, and it is nearly unusable: an Actions artifact cannot be downloaded
anonymously even from a public repository, so there is no `curl` on the target box,
only a signed-in browser or `gh run download`. It arrives zipped, it expires after
90 days, and it has no stable address: you navigate to the newest green run to find
it.

**So every master push that passes CI republishes a `rolling` pre-release** carrying
the binary and its SHA-256. One URL, no login, no expiry, no zip. The per-run artifact
stays, because a pull request still needs its own build and `rolling-build` fires on
master only.

**It waits for every gating job, not just the one that produced the binary.** Depending
on `static-binary` alone would have been enough to get the file, and would have
published a binary from a commit whose tests failed: that job proves the thing compiles
and links statically, nothing about whether it works. Anything invited into a `curl |
chmod +x` on somebody's server has to clear the same bar as the rest of the tree, so
every gating job is in `needs`, SonarQube included: once it waits for its own Quality
Gate it is a gate like the others, and leaving it out would publish a commit the branch
protection would refuse to merge.

**The tag moves, which this repository otherwise refuses to do.** Actions here are
pinned by commit precisely because a tag can be moved under you. A rolling build is
the one case where that mutability is the feature rather than the hazard, and the
distinction that makes it safe is who is trusting what: CI pins actions because it
must get the same code twice, whereas a person fetching `rolling` is asking for
whatever is newest. The release body names the commit it was built from, so the
bytes are still attributable after the tag has moved on.

**The release is moved and overwritten, never deleted.** The obvious way to move a
tag is to delete the release with its tag and make it again, and it has a window in
the middle where the URL 404s. Overwriting keeps the release object alive instead, so
the worst a half-finished publish leaves is a `rolling` that is stale rather than one
that is missing. It costs one awkwardness, that a release ignores the commit it is asked
to point at once its tag exists, so the tag is moved through the git refs API rather
than through the release.

**Replacing an asset needed the same care, and nearly did not get it.** `gh release
upload --clobber` reads like an overwrite and is not one: it deletes the existing asset
and then uploads, and its own help says the original is lost if the upload fails. Used
plainly it would have reintroduced exactly the hole the paragraph above avoids, on the
one file the whole feature exists to serve. So each new file is uploaded under an
`.incoming` name first and takes over only once it has landed whole: what is serving is
never removed for something that has not arrived. The remaining window is a rename, not
a transfer.

**Immutable releases had to be turned off, and the tag is called `rolling` because of
it.** GitHub's repository setting of that name freezes a published release: its assets
and its tag can never change. That is the right setting for a release and the exact
opposite of this one, and the collision is not a detail to work around, it is the two
features meaning contradictory things. The first live run proved it in the loudest
possible way, publishing a release with the correct notes, the correct target, and no
binary at all, because the upload was refused with `422 Cannot upload assets to an
immutable release`. The setting is now off.

**Deleting that release did not undo it.** The tag name stays burned: GitHub refuses to
create `nightly` again, saying the name `was used by an immutable release`, and no
amount of deleting the release or the tag frees it. So the rolling build lives at
`rolling` instead, which is a better name anyway and a poor way to have arrived at it.

**Cost:** the tag-triggered release job that is still owed will not get immutability
either, unless the setting is turned back on at that point and the rolling build is
moved off releases entirely. That is a real trade and it is deferred, not solved.

**And a master push is no longer cancelled by the next one.** Publishing is a sequence
of writes to something outside the run, and this workflow used to cancel a superseded
run wherever it had got to, which made every window in that sequence reachable. Killed
between the upload and the tag move, it would leave the assets and the tag describing
different commits, quietly, with no red run to say so. Cancellation now applies to pull
requests only, where a superseded run genuinely has nothing worth finishing, and master
pushes queue. Overwriting instead of deleting is still worth having: it bounds the
damage from a publish that fails for some reason other than being interrupted.

**The write grant is on a job that does not compile.** `rolling-build` downloads the
artifact `static-binary` produced and calls `gh`; it never runs cargo. Putting
`contents: write` on the build job instead would put a token that can push to the
repository in the same process as the build script of every dependency in the graph,
which is a supply-chain hole opened for no gain.

**The bytes carry provenance, not just a checksum.** A SHA-256 published beside a file
answers "did this download corrupt", which is the easy half and the half nobody was
worried about. It cannot answer "did this come from rastro's own CI", because whoever
could replace the binary could replace the checksum in the same motion. GitHub signs a
provenance attestation at build time against a short-lived OIDC identity, binding the
artifact to the workflow and the commit, and `gh attestation verify` checks it without
trusting the release page at all. For a tool whose entire claim is a trustworthy record
of a server, and which the README invites people to run as root, publishing a download
with no answer to that question was not defensible.

**Rejected: nightly.link**, a third-party proxy that hands out anonymous URLs for
public-repo artifacts, needing no CI change and no write grant at all. It puts a
third party in the distribution path of a binary meant to run as root on somebody's
server, which is a poor trade for a tool whose entire claim is a trustworthy record
of that server.

**Cost, and it is not only the moved tag.** The workflow is no longer write-free, so
the guarantee that read the strongest is now a per-job claim. Release assets carry no
permission bits, so a downloaded binary still needs `chmod +x`, exactly as the zipped
artifact did. Watchers subscribed to releases may get a notification per master push;
at 0.0.0 that is nobody, and it would be a reason to reconsider later rather than now.
And a moving pre-release is a poor place to build habits: the eventual release job
must not inherit any of this, which is why `rolling` says in its own body that it is
unrelated to any released version.

# CI: which checks gate, and what keeps them current

Dated 2026-08-30. An audit of the workflow found a required check that could not fail
for the reason its name implies, and a set of pins nothing was watching.

## The required SonarQube check waits for the Quality Gate

The `SonarQube` job was required by the branch ruleset and could not fail on a quality
regression. The scanner uploads an analysis and exits; the verdict is computed
afterwards, server-side, and arrives as a *different* check posted by Sonar's own GitHub
App, which the ruleset did not require. Four green required checks therefore said nothing
about whether the gate had passed, while looking exactly as though they did.

**`sonar.qualitygate.wait=true` makes the job wait for its own verdict.** No ruleset
change: the required context keeps its name and its integration, and only stops lying.
Requiring Sonar's app check instead would have worked equally well for gating, and was
rejected for a mechanical reason: an Actions job always emits a check of its own, so that
route leaves two checks on every pull request where the point was to have one.

**Cost:** a SonarCloud outage is now a merge outage, where before it was invisible. That
is the correct direction for a gate to fail, and it is a real cost on a bad day.

## Renovate, not Dependabot, because the toolchain is a dependency too

Pinning actions by commit trades a moving target for a silent one: nothing announces
that a pin has gone stale, and several were whole majors behind before an audit looked.
Dependabot was written first and then dropped, because it has no mise ecosystem and would
have left `mise.toml` unwatched, which is the pin that decides what the compiler does.
Renovate covers the same two ecosystems plus mise, verified against its source rather
than its documentation: its mise manager parses the `[tools.rust]` table form and rewrites
only `version`, leaving `components` and `targets` alone.

**`helpers:pinGitHubActionDigests` guards the next action, not the ones already pinned.**
An action written as `<sha> # v4` is updated in place regardless, comment and all;
Renovate does not unpin what is pinned. The preset matters for whatever gets added later,
which would otherwise stay on a floating tag and erode the convention an entry at a time.

**Cost, and it is the reason this is a decision rather than a detail:** Dependabot is
GitHub's own and needs no grant, whereas hosted Renovate is a third-party app with write
access to the repository, added to a project whose supply-chain posture is otherwise
strict enough to pin every action by hand. The trade was accepted because an unwatched
toolchain pin is a standing risk and the app's blast radius is a pull request that still
has to pass the same gates as any other. Self-hosting Renovate as a workflow removes the
third party and costs a job and a token to maintain; it is the reversal to reach for if
that trade stops looking right.

# What a fingerprint costs the box it runs on

2026-08-31. Driven by a run on a production PostgreSQL development host that was killed
after **50m58s having produced nothing**. New Relic during it: 44–71% CPU, user-time
dominated, and I/O read bytes climbing 14 → 32 → 67 → 84 GB, at roughly 10.4M read
syscalls. At the observed rate, 51 minutes is around 355 GB, which is more than most root
filesystems hold.

## Metadata everywhere, content nowhere by default

`WalkPolicy::built_in()` shipped one rule, `/` → `Hashed(Sha256)`, so **every regular file
on every non-pseudo mount was opened and hashed on every run**. The arithmetic confirms the
mechanism rather than merely suggesting it: `io::copy` into a hasher cannot take a
kernel-offload path, because the sink is not a file descriptor, so it falls back to std's
8 KiB `DEFAULT_BUF_SIZE`. 84 GB ÷ 8 KiB is 10.25M reads against the 10.4M observed, which
is agreement to 1.5% and leaves no second cause to look for.

The shipped table is now `/` → `MetadataOnly`, and nothing is content-hashed at all.

**What narrowed is the reading, not the scope, and the distinction is the whole argument.**
The walk is still total over every mount that holds files, and every path it reaches is
still in the document. No state surface left it. So this is not the inclusion list the old
default was written to avoid: a tree the table says nothing about loses one attribute, not
its existence.

**Detection survives, and by more than it looks.** An ordinary write moves mtime, ctime and
usually size. **ctime has no userspace setter at all** — no syscall sets it arbitrarily —
so hiding a content change from stat needs `touch -r` or `cp -p` *and* a moved clock. That
is deliberate evasion, and rastro is not an intrusion detector: `README.md` disclaims
prevention and monitoring, and `docs/research.md` rejects AIDE as a dependency. Paying
355 GB of reads and a production incident for the one property the tool says it does not
have is the wrong trade.

**Two things follow that are worth more than the time saved.** The walk now reads no file's
contents, so it moves no file's atime and pulls no file data into the page cache — it cannot
evict the working set of the database it is fingerprinting, which is a harm that lands
*after* the tool exits with nothing connecting the two. And peak resident memory fell from 267 MB to
23 MB, measured, because an entry stopped being a twelve-key map.

**Cost, accepted knowingly:** a same-size, stamp-preserving rewrite anywhere on the box is
now invisible. The box that needs that caught is the box that should be running an IDS.

**This does not supersede [Config](#config-is-optional-opt-in-and-exclusion-only).**
The walk stays total and exclusion-only. A future reader must not read this as licence to
make the *walk* opt-in.

**It refines [An inode's timestamps](#an-inodes-timestamps-are-nanoseconds-since-the-epoch-and-atime-is-not-recorded).**
That entry's "there is no atime, because reading a file to hash it moves it" now describes a
read that no longer happens.

**And it leaves [Churn stops reporting what moves](#a-tree-that-churns-without-meaning-stops-reporting-the-attributes-that-move)
standing on its noise argument alone.** That entry called the six churning trees "a seventh
of the bytes on that box, while `/usr` is two thirds": a performance claim that no longer
applies. The list survives because it makes the stamps, the size and the inode volatile,
which is the difference between a quiet diff and one carrying two journals every run. Do
not retire it on the grounds that nothing is hashed any more.

**Content hashing returns as its own opt-in collector**, over trees the operator names,
where the cost can be consented to rather than discovered. The hashing seam — `sha256_of`,
`open_without_following` and its `O_NOFOLLOW`/`O_NONBLOCK` TOCTOU defence, and
`ContentPolicy::Hashed` — is deliberately kept and still covered by tests, because that
collector needs exactly it and the reasoning in those doc comments is the expensive part to
reconstruct.

## An entry is a digest of its metadata

Listing eleven attributes per path cost 444 bytes an entry, and 13 MB on a container of
30,891 of them, in a document that is 80% filesystem facet. An entry is now one digest of
those attributes: 81 bytes, 2.40 MB for the same host, and the signal check holds — a
`chmod` on one file moved exactly one digest, and the whole-document diff was four lines.

Since the document names every path on the box, its floor is the path strings themselves,
so a digest per path lands within a fifth of the smallest complete document there could be.
Everything below that floor costs completeness, and completeness is not for sale: Ansible
can touch anything anywhere, and the cascades — a package post-install script writing
somewhere nobody thought about — are the reason the tool exists.

**XXH3-64, and the reasoning is not "it is fast".** The inputs are ~80 bytes each and tens
of thousands of them, which inverts the usual intuition: blake3's throughput is a
large-input number and its initialisation dominates below about a kilobyte, so it measures
*slower* here than SHA-256 with SHA-NI, while XXH3 is built for exactly this shape. Roughly
0.6 ms against 2.3 ms for truncated SHA-256 and 7 ms for blake3, over 46,000 entries.

**Sixty-four bits, and that is not a compromise.** A digest is only ever compared with the
digest of the same path in another run, so a collision between two different paths means
nothing at all. The only failure is an entry that changed and hashed the same anyway, at
2⁻⁶⁴ per changed entry; even treating the digests as a set across 46,000 entries the
birthday bound is ~6e-11. Width is also what drives document size more than anything else
here, at four bytes of document per byte of digest.

**It cost a licence decision, which was not free.** `xxhash-rust` is BSL-1.0, and `deny.toml`
allows exactly the licences the tree already contained on the stated principle that "a new
licence is a decision, so it should fail until somebody makes it". CI duly refused it. BSL-1.0
is permissive, OSI-approved and FSF Free/Libre, with no copyleft and no notice to reproduce in
a binary, so it was allowed deliberately rather than worked around. The alternative was
truncated SHA-256 from the `sha2` already in the tree, at 2.3 ms against 0.6 ms — 0.4% of a
run, and no new dependency at all. Worth knowing as the cheap reversal if that trade ever
stops looking right.

**Not cryptographic, and it does not need to be.** Forging one means choosing a file's mode,
owner, size and stamps, which an attacker who can write the file already controls. What it
*does* need is to be identical forever, or a stored fingerprint stops being comparable —
which is why the crate is pinned and why `DefaultHasher` is disqualified whatever its speed:
std explicitly declines to keep its output stable across releases.

**Cost:** a moved digest says a path changed and not which attribute did. `--detail` records
all eleven instead, and has to be asked for at the time, because a summary taken yesterday
cannot be expanded today.

## The digest covers exactly what the view would have shown

A digest over a directory's derived stamps would move whenever a child appeared, and one
over a churning tree's size and inode would move on every run. Either would end the
byte-identical guarantee at the one facet that dominates the document, so the digest is
taken over precisely the attributes that survive the view's volatility filter.

Volatility is therefore load-bearing for the digest, not decoration on it, and the work in
[Churn stops reporting what moves](#a-tree-that-churns-without-meaning-stops-reporting-the-attributes-that-move)
and the derived-stamp rule are what make this possible at all.

**A withheld attribute and an absent one are different bytes**, because otherwise a
churning file with its size withheld and a directory that has no size would agree.

**The policy the entry was read under is deliberately not in the digest.** It is rastro's
configuration rather than the box's state, so folding it in would report a changed config as
a change to every file on the host. The effective table in the `invocation` facet is where a
reader learns which rule applied.

**Tension, stated rather than resolved away:** this makes the *collector* compute something
that depends on how the document will be rendered, which sits awkwardly beside
[collectors annotate, renderers act](#collectors-annotate-values-renderers-act-on-them).
It is accepted because the alternative — a renderer that knows what a file entry is — is
worse, and because the digest is an observation about a path rather than a presentation of
one.

## A path that is gone is omitted, a path that will not be read is recorded

The walk propagated `?` on every stat, readdir and digest read, so **one unreadable or
vanished path failed the entire facet**. On a busy host a log rotating mid-walk is not an
edge case, it is a certainty, and so is `EACCES` on a fuse mount.

- **Absent (`NotFound`, `StaleNetworkFileHandle`) → the entry is omitted.** This is what
  keeps the byte-identical guarantee true: a file that rotated away between two runs must
  not appear in one document and not the next for a reason that is not a change to the box.
  It is not a silence violation either — absence is state, and a path that was not there
  when the walk arrived is honestly reported by the same absence as one that never existed.
- **Everything else → the path is recorded with the reason.** `EACCES` and `EIO` reproduce
  at the same path on every run, so they diff cleanly and belong in the *default* view as
  the lasting blind spots they are.

`ErrorKind` is `non_exhaustive` and std still maps `EIO` to a kind no stable code can name,
so the default is "not an absence": the direction that records too much rather than omitting
a path that is really there.

**An entry is its attributes or the reason it has none, never a partial set.** That is the
facet's own `data`-or-`error` contract one level down. A directory whose listing fails
therefore loses its own recorded mode and owner, which is the field an operator would change
to fix it — accepted, because the alternative is an `unlisted_because` key that is null on
45,951 entries out of 45,952.

**One failure stays fatal:** the root's own stat, because a walk that cannot start is not a
host with no files on it.

This refines [A contested tree fails the facet](#a-tree-two-collectors-claim-fails-the-filesystem-facet)
one level down: that entry established "the facet, not the run"; this one establishes "the
entry, not the facet".

## A name that will not decode is reported, not fatal

Linux paths are bytes and the document holds text, so a name like `b"\xff"` is legal on disk
and unsayable in a fingerprint. Substituting `U+FFFD` remains refused, for the reason
`canonical_tool` refuses it for a tool's output: it would put a path into the document that
is not on the box, and one nobody could act on.

The old answer was to fail, on the argument that a path with no name has no key to be filed
under. **The argument was right about keys and wrong about reporting.** It cost the entire
`filesystem` facet — every path on the box — for one extracted archive with a mojibake name,
and the document did not say which file it was.

So such an entry is now reported in a list of its own rather than keyed: the name's bytes as
lowercase hex, which claims nothing and is exact, and the directory holding it as the text it
is. The two together reconstruct the path exactly. The walk does not descend into it, because
every name beneath an unnameable directory is unnameable too and the directory is the fact.

**Found the hard way**, which is why it earned an entry rather than a backlog line: the test
that documented the old behaviour left its one-byte fixture inside `CARGO_TARGET_TMPDIR`,
which a walk of the real host covers, and it silently refused the `filesystem` facet of every
later run in the suite. The determinism harness went on passing, because two runs failing
identically are still identical.

## The fingerprint goes to a file by default, and stdout only when asked

A fingerprint of a real host is megabytes, and a default that puts megabytes on a terminal
punishes the first run. Worse, the 51-minute run above produced *nothing at all*: the
document was built in memory and printed at the end, so an interrupt threw away the work.

The default is now `./rastro-<host>-<UTC>.json`, and `-o -` restores the pipe.

**This does not weaken "stdout carries only the fingerprint"** (`CLAUDE.md`): with the
document in a file, stdout carries nothing at all. It fills in
[v1 scope](#v1-scope-generate-only-current-box), which already said "to stdout or a file",
and reverses only the wording of `docs/design.md`'s Streams section.

- **No colon in the instant.** A name carrying one needs shell quoting, breaks on VFAT and
  exFAT, and reads as a host separator to `scp` and `rsync`.
- **One clock reading serves the filename and `started_at`**, so a file cannot disagree with
  the document inside it. The hostname likewise. Both are read in the composition root and
  handed to the collectors, which finally makes `seconds_since_epoch`'s doc comment true.
- **The hostname is untrusted input.** It comes from `/proc/sys/kernel/hostname`, which is
  settable, and rastro runs as root — so `../../etc/cron.d/evil` would steer the default path
  out of the working directory. Anything but `[A-Za-z0-9._-]` is dropped, the result is
  capped, and a hostname that survives as nothing is omitted exactly as an unreadable one is.
- **Created `0600` at creation, not chmod'd afterwards**, so there is no window in which a
  document naming every path on the box is world-readable. This keeps a promise
  `docs/design.md` had listed as unbuilt.
- **Temp sibling, then rename.** `README.md` already promises that a run which died halfway
  cannot leave half a document to be diffed. `--force` overwriting in place would also
  silently keep an existing file's 0644, because a mode applies at creation.
- **An existing file is refused unless `--force`.** The workflow is a `before` and an
  `after`, so replacing the `before` destroys the only record of the state being compared
  against. That is the one irreversible thing this tool can do to an operator.
- **`rastro-ssh` passes `-o -`.** Without it every remote run would leave a document in the
  remote working directory — root's home on most boxes, and walked — and return nothing.
- **A destination that is not a regular file is written *through*, never published over.**
  Found in review, and it mattered: the first version staged and renamed unconditionally, so
  `rastro -o /dev/null` as root replaced the null device with a regular file, and `-o
  /dev/stdout` would have replaced its symlink. A stream has nothing to make atomic anyway.
- **Without `--force` the refusal is the kernel's.** The overwrite check and the publication
  are separated by however long the document takes to render, so a check-then-rename could
  replace a file that appeared in between — the one thing this is here to prevent. Published
  with `link`, which fails `EEXIST`, rather than `rename`, which would take it. Also found in
  review.

## The output file is left out of the walk, and the invocation facet says so

Run one writes a document; run two's walk finds it sitting there. So the most natural use of
`-o` — the same path twice, which is exactly the before-and-after workflow — broke the
byte-identical guarantee by a megabyte.

The resolved output path is therefore omitted from the walk, through the same seam that omits
a staged binary, and declared in the `invocation` facet beside `observer`. Volatile for the
same reason that one is: the path carries a timestamp. This refines
[Only a staged run omits it](#only-a-staged-run-omits-the-binary-and-the-caller-says-so):
same principle, same seam, a second path.

**Reproduced by accident before it was designed**, which is the only reason it was caught:
a measurement script wrote three fingerprints into `/` and the last two differed. Written to
tmpfs, which the walk skips, all three were byte-identical.

**The path has to be resolved, not merely made absolute.** `std::path::absolute` is lexical, so
`-o linked/fp.json` through a symlinked directory keeps the symlink — while the walk never
follows one and meets the file under its real directory. The two spellings would not match and
the document would land back in the next run, silently, for the workflow this entry exists to
protect. Resolved once in the composition root and handed to both the walk and the facet, so
there is one answer rather than two. Found in review.

## Progress is a counter, not a bar, and only on a terminal

The 51-minute run gave no sign of life. There was no way to tell whether it was working,
where it had got to, or how much longer — so the only available action was to kill it.

**No percentage and no ETA, and that is a decision rather than a shortfall.** The walk
discovers its own work as it goes. The one cheap denominator is the used-inode count per
mount, which needs `statfs` — a syscall std does not wrap, so reaching it would cost the
workspace's `unsafe_code = "forbid"` — and even bought, it would bound entries rather than
time. A number
that slides smoothly and means nothing is worse than an honest count.

So: a live single-line counter of the current collector, entries walked and elapsed, gated on
`stderr` being a terminal. The gate is what keeps "a clean run says nothing on stderr" true
by construction rather than by anybody remembering it, and `--progress` / `--no-progress`
force it either way. The line is cleared before any diagnostic, so a warning is never
half-overwritten by a counter.

## Timings are told to the operator, never written into the document

`--debug` reports per-collector wall clock, what the walk read, where the document went and
peak resident memory, on stderr. It exists because `time ./rastro > file` answers neither
"which collector was slow" nor "where did the document go", which are the two questions a
slow run actually raises. It earned itself immediately: on a first measured run the
filesystem walk was 7.747 s of 7.761 s, which is 99.8% and settles where any further
optimisation has to go.

A duration must not reach the document.
[A configured endpoint is a different fact](#a-configured-endpoint-is-a-different-fact-from-a-bound-socket)
already establishes that a fingerprint records what a box *is*, not what it is doing, and a
timing would have to be volatile anyway, so the default view would drop it.

**The seam is what makes that structural.** `rastro-collector` gains a `RunProgress` trait
whose methods say only *what* happened; the tool holds the clock. The library is handed no
clock at all, so it could not write one into a facet if it wanted to — which is a stronger
guarantee than the purity test that forbids `SystemTime::now` there. Registration order, not
slowest-first, so two `--debug` runs are comparable line by line.

## The run is estimated before it starts, and warned about, never limited

A budget the operator has to tune presupposes they have already investigated the box, which
is the work rastro exists to do. So the pre-flight estimates and warns: inodes in use across
the local filesystems against free space where the document is going, and a line on stderr
only when the document would be a real fraction of what is free.

Through the hardened `canonical_tool` seam to `df` rather than `statfs`, for the same
`unsafe` reason as above and because a bounded subprocess buys the number at no cost to a
stated property.

**Honest limits, all of them:** `df -i` counts every inode on a filesystem including those
under a tree a collector sealed, so a two-million-file PostgreSQL cluster inflates it and the
walk will never touch them. Over-estimating is the right direction for a warning — it can cry
wolf, where an under-estimate would stay quiet about the run that fills the disk. A filesystem
with no fixed inode table reports `-` where the count would be — vfat does, so `/boot/efi`
prints `0 0 0 -` — and that row is skipped rather than read as zero. A box without `df` gets no
guess rather than a made up one. And it bounds entries, not wall time.

**There is deliberately no special case for a host that counts nothing at all.** `--local`
always lists `/`, and on a real box udev and several tmpfs besides, every one of which reports
a count, so the sum is never zero on a host that could have run rastro. Guarding a state the
box cannot be in is complexity presenting itself as safety, and it costs a branch no fixture
can honestly justify.

## The renderer streams, and the document is never copied to filter it

Rendering built four full copies at peak: the collectors' tree, a recursive deep clone from
`in_view` — paid even for `View::Complete`, where nothing is filtered — a second complete
copy as a `serde_json::Value`, and the rendered `String`. Measured at 267 MB resident for a
13 MB document, which is 8.6 KB of memory per 444 bytes of output.

The attribution is worth recording, because the ratio is the tell rather than the total:
`Content::Object` is a `BTreeMap`, whose nodes allocate a fixed eleven key and eleven value
slots whether used or not, so a twelve-key entry needs two leaves and an internal node —
about 2.5 KB of container to hold 444 bytes of data, four times over.

Now: `to_canonical_json_writer` writes straight into a `BufWriter`, and the observation tree
is serialised where it lives through a borrowed filtered view. **The view rule stays in the
domain**, expressed as `visible_in` returning a borrow rather than a copy, so the renderer
still never asks whether anything is volatile — it just no longer owns what it is given.

Sortedness now comes from the domain's own `BTreeMap` rather than from `serde_json::Map`,
which retires the `preserve_order` hazard entirely: no map of serde_json's is involved.

**Proved byte-neutral rather than argued to be.** Two golden tests pin the exact bytes of a
document covering every `Content` and `Scalar` variant, a nested object, a list, a dropped
volatile leaf and a dropped volatile subtree, in both views — written before the change, so
that neither the `Value` hop nor the clone could go without proving it cost nothing.

## Considered and rejected: posix_fadvise, to give the page cache back

Reading 355 GB through the page cache evicts the working set of the database being
fingerprinted, and the resulting latency lands *after* rastro has exited with nothing
connecting the two. `posix_fadvise(DONTNEED)` after each file would limit that.

**Rejected, because the problem was removed at the source instead.** With nothing
content-hashed the walk reads no file's contents, so the harm falls by four orders of
magnitude and there is nothing left to advise about. It was never a complete fix either: `DONTNEED` cannot
distinguish a page rastro brought in from one the database already had hot, so it evicts the
database's pages either way, and it does not write back dirty pages at all.

It also has a price beyond its own code. std does not wrap the call, so reaching it means
either an `unsafe` block — against the workspace's `unsafe_code = "forbid"`, and against
`docs/design.md` advertising the unsafe-free build as one of only two security properties
that are true today — or a fourth runtime dependency for one syscall. Neither is worth
paying for a mitigation of a problem that no longer exists.

**Revisit if** a future opt-in content collector reads enough to matter, or if a host shows
cache pressure after a run.

## Considered and rejected: the filesystem's own record of what changed

Every mechanism cheaper than walking requires something arranged *before* the change, which
is precisely what rastro exists not to require.

**inotify** reports only while it is watching, has no recursive mode — so adding a watch per
directory means walking the tree anyway, bounded by `max_user_watches` — and cannot see the
past. **fanotify** with `FAN_MARK_FILESYSTEM` fixes the scalability half with one mark per
filesystem, and neither fixes the other half: both need a daemon running across the interval,
which is the thing `README.md` refuses to be.

**A journal is the wrong structure, despite the name.** ext4's jbd2 and XFS's log are
write-ahead logs for crash recovery: circular and continuously overwritten, so on a busy host
they wrap in minutes; recording *block* modifications rather than path operations; with no
userspace API, so reading one means the raw device or `debugfs`, which is unsafe on a mounted
read-write filesystem; and discarded at checkpoint. ZFS's ZIL is the same in every relevant
respect.

**Copy-on-write is the right structure, and is a different feature.** btrfs generation numbers
and ZFS block birth times are *persistent* metadata, which is why `btrfs subvolume find-new`
and `zfs diff` work — and the latter reports renames, which a metadata digest cannot detect at
all. Filesystem-specific, though: ext4 and xfs, which is what a Debian host is, have nothing.

Two closer fits exist and neither is retroactive: the kernel's **IMA** subsystem maintains a
measurement list but needs boot-time policy, and **auditd** with `-w /etc -p wa` is a genuine
change log that many compliance-managed boxes already run. Worth checking for on a host, not
something rastro can arrange after the fact.

**Worth revisiting as a Layer-3-style specialisation**: if a run finds btrfs or ZFS with a
usable prior snapshot, it could narrow the walk. It would be optimising a step that now costs
about a second.

## A test that is not about the walk does not pay for one

The suite went from about a minute to eight. Not one slow test: the binary is invoked around
forty-five times across `cli.rs` and `output.rs`, and every invocation walked the whole runner
— a cargo registry and a coverage-instrumented target directory, hundreds of thousands of
inodes — through an instrumented binary.

So the tests that never read the `filesystem` facet now pass a config that excludes it. The
ones that are actually about the walk still pay for it, and so do the three that assert a clean
run says nothing on stderr, since an exclusion prints a WARN there.

**Then measured again, because excluding the facet was the blunt version.** CI's step timings
showed four tests over 120 seconds *each* on the runner, and the fix was the config feature this
same change added: `sealed` over the root and the shipped churn trees leaves one entry per mount
root — a `filesystem` facet that is genuinely `ok`, rendered through the real walk, in about ten
milliseconds. Sealing the root alone is not enough, because a shipped rule for a tree inside it
is more specific and still descends.

That is better than excluding the facet, not merely faster: the test still exercises the walk,
the rendering and the digest. It also prints nothing on stderr, because a narrowing is not an
exclusion, which is what lets the "a clean run says nothing" tests use it.

**Then the last two were decomposed rather than accepted.** Proving the walk leaves out the
document it is writing needed the document to be *in* a walked tree, which through the binary
meant walking a whole host — over two minutes each on the runner. Asserted instead over a
scratch root through `FilesystemCollector::walking`, where the walk is scoped and the omission is
the only difference, it is instant *and* a sharper claim. What only an end-to-end run can show is
that one resolved path reaches both the walk and the envelope, and that needs no walk at all.

Two tests still pay for a real walk, and each has a reason that sealing would destroy: one
asserts what a run with *no* config looks like, and one proves a config narrowing by checking
that a sibling of the sealed tree is *still* walked. Both are worth their seconds.

Local suite: 64 s before any of this, 48 s after excluding the facet, 26 s after sealing,
**13 s after moving the omission proofs to a scoped walk**. On CI the step went 311 s to 207 s
at the sealing stage, measured; the decomposition lands after.

**Excluding the facet also removed a source of flakiness, which is the more interesting half.** An instrumented
run writes a `.profraw` into the target directory, and the runner writes its own worker log
while the suite runs — both inside the tree a walk covers. Two runs of "an unchanged host" were
therefore never comparing an unchanged host, and the determinism harness failed on files the
test harness itself had created.

**Which exposed something worse: that harness had been passing vacuously on CI.** CI runs
unprivileged, `/boot/efi` is unreadable there, and until the error-tolerance work a single
unreadable path failed the whole `filesystem` facet — so both runs errored identically, the
bytes matched, and the facet that dominates the document was never compared at all. It has been
that way since the collector was registered. Two runs failing the same way are still identical,
which is the blind spot in comparing bytes and nothing else.

The property itself holds and is verified where it can be: five consecutive runs to one path on
the reference box, byte-identical, with the walk included.

**So the harness was split rather than weakened.** The end-to-end test compares the envelope
and the other twenty facets through the real binary, which is what it can honestly assert on a
machine that is being used. The `filesystem` facet's byte-identity moved to a test over a tree
it owns, where it can assert something the whole-host version never could: that a churning
tree's size and stamps genuinely moving between two readings leaves the diffable view
identical, with a counterweight proving a real change still shows.

Asserting whole-host byte-identity on a busy runner would be asserting that nothing on the box
moved during the test, which is neither rastro's promise nor true. The promise is verified where
it can be: five consecutive runs to one path on the reference box, walk included,
byte-identical.

## A tool's `null` is not a broken document

`systemctl list-timers --output=json` reports `"activates": null` for a timer that starts
nothing systemd can name. A Debian 12 box writes `""` for the same case, which is what the field
was measured against and why it was typed as a `String` with `serde(default)` — and `default`
covers an *absent* field, not a null one. So on a GitHub Actions runner one such timer failed the
entire `timers` facet, on every run of that host.

Absent, `null` and `""` now converge on one value before anything decides what it means.

**Found by improving a failure message rather than by guessing.** The determinism harness
reported `facets/timers differs` with both sides printing `data: null`, because the reporter only
ever printed `data` — and an `error` facet has none. Two runs failing identically are still
identical, so it read as a flake. Printing `status` and `error` turned it into two messages that
differed only in a byte offset, which is the giveaway: the failure was constant and its *text*
varied, because the offset lands after timer clock values whose width changes.

Worth recording that the first hypothesis was wrong. The timing suggested the concurrency change,
so the guess was contention between two collectors both shelling out to `systemctl`. Measurement
killed it: 120 runs on a systemd host with no failure, 40 of them at four times the subprocess
pressure, and `units` makes two `systemctl` calls rather than the one per unit that had been
assumed. A cheaper diagnostic would have been quicker than the reasoning.

**Still owed, and the same class three times over:** one unreadable mount point, one
undecodable filename and now one null field have each cost a whole facet. The blast radius is
the problem rather than the strictness — a row that will not parse should cost that row, as a
path that will not read costs that path. There are 27 non-optional scalar fields across seven
JSON deserialisers with the same exposure, and making them all optional would trade one
fragility for a weaker type. Per-row tolerance is the consistent fix and it is not in this
change.

## A config can narrow the walk, and the operator outranks a collector

The only lever over which trees the walk read was a collector's claim, resolved from the host.
So the 51-minute run was unfixable without a new binary, and CI had no way to say that its own
build directory is noise. `docs/config.md` recorded that as a known gap; this closes it.

Three keys — `metadata_only`, `churns`, `sealed` — and **deliberately no `hashed`**. All three
withhold, so this is not a new principle but the existing one reaching the operator instead of
only reaching collectors: a config may narrow and never widen. The type is what enforces it, as
`ClaimedReading` already does for a claim — `Config` has nowhere to put a fourth reading, and
`deny_unknown_fields` turns an attempt at one into an error rather than a line that silently
does nothing.

**The operator's rule beats a collector's claim, which is the opposite resolution from a claim
meeting a claim.** Two collectors naming one tree is a bug in a collector pair with no way to
pick a winner, so it fails. An operator and a collector naming one tree is an operator
correcting rastro: a claim is rastro's reckoning about a tree from the outside, and the operator
knows their box. Proven on the reference box, where a config rule over
`/var/lib/postgresql/17/main` replaced the `postgresql` collector's claim and the effective
table changed its `claimed_by` to `config` — the same pair would previously have been a hard
conflict.

Two config rules for one tree is still refused, for the reason a shipped table naming one tree
twice is: the operator meant one of them.

**Declared, never silent.** Each rule renders in the `invocation` facet with `claimed_by:
"config"`, so a reader of a tree with no entries can tell rastro's reckoning from a colleague's
config file. `config` is not a facet and no collector may be called that; it is spelled as a
claimant anyway because the question the table answers is who decided.

**A bad path fails the facet, not the run.** An operator's typo should not cost them every other
facet on a box they were trying to inspect, which is the same rule a claim conflict follows.

Measured on the reference box: sealing `/usr/share/doc` and the cluster took the document from
45,993 entries to 42,965 and 4.63 MB to 4.41 MB, at 0.43 s.

## nextest runs the suite, and each test gets its own process

`cargo nextest` rather than libtest, measured on the reference container: **43 s against 64 s**
for the whole suite, and the same under instrumentation. CI runs `cargo llvm-cov nextest`,
which writes the same lcov report to the same path, so the SonarQube import is untouched.

**Process isolation is worth more here than the time.** A test that invokes the real binary is
observing the machine the suite runs on, so tests interfere through the filesystem rather than
through memory — and one panicking test cannot take its neighbours down with it.

**A fixture asserts the modes it created.** A file written without saying so takes whatever the
umask allows: 0644 under the usual 022, 0664 under the 002 a fresh Debian user gets. Three tests
asserted a literal mode they had never set, so they passed for the GitHub runner and failed for
anyone whose umask differed. The fixture sets file and directory modes explicitly now.

**It found a latent race immediately.** The accounts fixture named its scratch directory from a
`static AtomicUsize` counter, which is unique per *process*. Under libtest the whole binary is
one process, so that held. Under nextest every test is its own process, so all of them started
at zero, chose `accounts-0`, and `remove_dir_all`'d it out from under each other. The two config
files the tests write had the same shape of problem, benign only because the contents matched.
Both are keyed by process id now.

**A failure names the destination, never the staging file.** Also found by running the suite as
somebody else: an operator who typed `-o closed/before.json` was told about
`closed/.before.json.1234.partial`, a file they never chose and cannot act on. And the test that
would have caught it skipped as root, so it had only ever really run on CI.

**Considered and measured: serialising the tests that walk the whole host**, on the reasoning
that they contend for one disk and write into the tree the others are walking. It cost 77
seconds — 120 s against 43 s — and bought nothing, because nothing asserts byte-identity across
the whole host any more. Recorded in `.config/nextest.toml` so nobody adds it back on the same
reasoning without measuring it.

## Collectors run concurrently, and the walk runs alone

`--debug` measured where a run actually goes on the reference box: the filesystem walk 0.145 s
of 0.839 s, and the rest waiting for subprocesses to answer — `exporters` 0.33 s, `postgresql`
0.31 s, `units` 0.29 s. So 83% of the run was latency, one tool at a time.

Collectors now run on a pool of four. Measured: **0.839 s → 0.455 s**, with those same three
collectors summing to 0.94 s of work inside a 0.45 s run.

**Four, not one per core.** Almost every collector spawns a subprocess, and a fingerprint that
starts twenty tools at once on a production box is an intrusion of its own — the thing this
tool exists not to be. The wait is latency rather than CPU, so a small pool recovers nearly all
of it.

**`Collector: Send + Sync`, which is a breaking change to the published port and cost nothing.**
All 22 built-in collectors satisfied it already: each holds validated owned values, detection
happens eagerly at construction rather than being memoised, and there is no interior
mutability anywhere outside the progress sink. An out-of-tree collector that holds an `Rc` or a
`RefCell` will have to change, which is the price.

**The filesystem walk declares itself `Exclusive`, and this is the substance of the entry.**
It is the one collector that can notice the others: it observes every mount, so a temporary
file another collector's subprocess created and deleted *while it walked* would be recorded in
one run and not the next. That is the byte-identical contract gone, for a second saved.
Running collectors one at a time made it impossible by accident; running them together makes
it possible, so the walk now says it needs the box to itself and runs last, alone.

The cost of that isolation is nothing measurable, because the walk was 17% of the run and the
other 83% is what overlaps. Verified: five consecutive runs to one path on the reference box,
byte-identical.

**Two hazards checked rather than assumed.** The `processes` collector annotates every process
volatile, so catching a sibling's subprocess in `/proc` cannot reach the default view. And
`subprocess` creates its pipes with `pipe2(O_CLOEXEC)` and puts each child in its own process
group, so concurrent spawns neither leak descriptors into each other's children nor kill each
other's tools on a timeout.

**What a caller is told changed shape.** Progress callbacks fire in completion order, from
whichever worker got there, because the point of hearing that a collector started is to say so
while it is still running. The `--debug` table is therefore sorted **by name** rather than by
registration or by cost: name order is deterministic and matches the document's own, which is
what makes two runs comparable line by line. The live counter shows how many collectors are in
flight rather than naming one, since naming one of four would be a lie.

Also removed here: `WalkProgress::file_opened` and `bytes_hashed`, declared but never called
and structurally zero since nothing opens a file any more.

# The network facet, against a box nobody set up for rastro

Dated 2026-08-31. Driven by the first run against a production PostgreSQL host,
where 21 collectors reported `ok` and `network` reported `error`.

## `ip` is asked for details, because it hides a route's defaults

`ip -4 -j route show` reported a default route with no `protocol` key, so the
`RouteObject` deserialiser failed on a required field and the whole facet was lost
— interfaces, both routing tables, everything, on a host whose networking was
entirely ordinary.

It is not a quirk of that box. `print_route` in iproute2 prints `protocol` only
when the kernel's `rtm_protocol` is not `RTPROT_BOOT`, and `scope` only when
`rtm_scope` is not `RT_SCOPE_UNIVERSE`, unless details are switched on:

```c
if ((r->rtm_protocol != RTPROT_BOOT || show_details > 0) && filter.protocolmask != -1)
        print_string(PRINT_ANY, "protocol", "proto %s ", ...);
```

`RTPROT_BOOT` is what an `ip route add` that named no protocol leaves behind, which
is every static route ifupdown installs. So the *default* case is the one with no
protocol to read, and the collector could only ever have worked on a box whose
routes came from DHCP, NetworkManager or systemd-networkd. The development box was
one, which is why the fixtures were.

**The invocation now asks for `-d`**, rather than inferring `boot` from the absence.
The inference would be sound for the way rastro invokes `ip`: the other two
suppressors are a `proto` filter rastro never passes and `RTM_F_CLONED`, which only
appears in the route cache rastro never reads. It is still reasoning about another
program's print policy to supply a value rastro did not observe, and this is the
decision already recorded for [`-j` over parsing tables](#collectors-ask-their-tool-for-json-which-promotes-serde_json-to-a-real-dependency):
prefer the source whose shape rastro chooses over the one it has to infer. Asking
also fixes `scope`, which was not failing but was recording `None` for every global
route, quietly spelling "global" as "`ip` said nothing".

`protocol` stays a required field. Required is what makes a future `ip` that stops
answering the question a loud failure rather than a route carrying a protocol nobody
observed.

**The collector's version went to `2`.** On identical host state the facet now
reports a route it previously failed on, and a `scope` where it reported none, so a
consumer diffing across the change has to be able to see that the collector moved
rather than the host.

**Cost:** `-d` also emits `"type":"unicast"` on every route, which rastro ignores.
That is real state — `blackhole`, `unreachable` and `local` are meaningfully
different from `unicast` — and recording it is a format addition rather than part of
this fix.

**The same class, now four times over.** [A tool's `null`](#a-tools-null-is-not-a-broken-document)
counted three: one unreadable mount point, one undecodable filename, one null field,
each costing a whole facet. This is the fourth, and it is one of the 27 non-optional
scalar fields that entry named as carrying the same exposure. Per-row tolerance was
the fix it deferred, and a second production facet lost to a single row is the
argument for stopping deferring it. Not in this change either.

**What the test does, since the fixtures were the hole.** A fake `ip` emulates
iproute2's suppression policy rather than replaying one host's output, so the test
asserts the question rastro asks. A fixture of the real `-d` output proves `boot` and
`global` are read, and one of the real output *without* `-d` proves the failure stays
loud if the flag is ever dropped. The container that gates CI would not have caught
this: netavark installs its default route with `proto static`.

# A second architecture, because the target host was one

2026-09-01. `docs/decisions.md` had promised aarch64 since the form was chosen, and
`rastro-ssh` documented `./rastro-aarch64` in its usage line, but CI built one triple and
`rolling` published one asset. The gap surfaced the first time a target host was an arm64
Debian 12 guest.

## aarch64 is built on an arm64 runner, not cross-compiled

Three routes reach an aarch64 asset: `cross`, `cargo-zigbuild`, or a native arm64 runner.
The first two build on the x86 runner already in the workflow and add a container or a
second compiler driver; the native runner adds a runner class instead.

**The smoke run decided it.** `static-binary` runs the binary it just built and asserts
the `file` output says `static`, and neither assertion is available to a machine that
cannot execute what it produced. A cross-compiled aarch64 asset would be published on the
strength of a target triple and a linker exit status, which is exactly the trust the
existing job was written not to extend. On a native runner both legs are the same six
steps with the triple substituted, so there is one job and no second toolchain to keep
current. Verified on an aarch64 musl build: `file` reports `statically linked`, so the
assertion that already gates x86 needs no arm-specific spelling.

**The matrix is the only target list.** The artifact name is the published asset name, so
`rolling-build` derives what it publishes from the directories it downloaded and holds no
triple of its own. Adding a third architecture is one matrix entry.

**A matrix renames the check, so an aggregate job keeps the old context.** The branch
ruleset requires `musl static build`. A matrix reports one check per leg under a name of
its own, so matrixing the job retired that context silently, and a required check that is
never reported blocks every pull request rather than failing one. A one-step job carrying
the old name and `needs: static-binary` restores it, with `if: always()` load-bearing:
without it the job is skipped along with a failed or skipped dependency, and a skipped
required check does not block a merge the way a failed one does. The context now says
more than it used to, since it passes only when every architecture built. Editing the
ruleset to require the two new contexts instead was rejected for the reason
[the Quality Gate entry](#the-required-sonarqube-check-waits-for-the-quality-gate) already
gives: a required context that keeps its name outlives the job layout behind it.

**Cost, and it is why this is an entry rather than a detail:** GitHub's arm64 runners are
free for public repositories and billed for private ones, so the build now depends on this
repository staying public in a way it did not before. `ubuntu-24.04-arm` is also a pinned
image where the x86 leg tracks `ubuntu-latest`, because there is no `-latest` alias for
arm; that pin needs moving by hand when it ages out. The reversal, if either cost stops
looking right, is `cargo-zigbuild` on the x86 runner, giving up the smoke run.

**What is not covered.** `check` still runs the test suite on x86 only, so the determinism
harness has never gated on arm. A first manual run on aarch64 Debian 12 produced a
complete document, five facets erroring loudly on an unprivileged user's permission
denials and the rest `ok`.

# Governance: the gates a contributor meets, and the ones CI meets for them

Dated 2026-09-01, from an audit of what a contributor meets before they run
anything. The pipeline held up. What was missing was the documentation around it,
and two gates whose absence nothing announced.

## The container gate is a workflow of its own, opened by a label

`CLAUDE.md` has said since Layer 1 landed that the real gate is a Linux container,
Alpine as well as Debian, unprivileged as well as root. Nothing enforced it. CI ran
one distribution as one unprivileged user, and the musl binary was asserted static
and then only asked its version, on a host with a glibc.

Adding it to `ci.yml` was rejected on cost: two images pulled and the workspace
compiled four times, in front of every pull request, for a gate that most changes
cannot break. It lives in `container.yml` instead, on every push to master, nightly,
on demand, and on any pull request carrying the `container` label. `ci.yml` carries a
comment saying which changes should add the label, because a label nobody knows to
apply is the same as no gate.

**One script, called from both places.** `scripts/container-suite.sh` runs inside the
container; `scripts/test-in-container.sh` is what a working machine calls, and the
workflow calls the same inner script. The recipe was documentation before, which is
the form that drifts: what CI asks and what a contributor asks are now the same file.

**The unprivileged half is not optional and has to be explicit.** A container job is
root by default, which is the opposite of the runner it replaces, so the script
creates a user and runs the suite a second time as them. Three defects have already
hidden in that difference: a test that skipped itself as root, a mode assertion that
depended on the caller's umask, and an unreadable mount point that failed a facet.

**Cost:** the images float rather than being pinned by digest, unlike every action
here. Deliberate, since the question is whether rastro works on today's Debian and
today's Alpine, but it means the nightly run can go red for a reason no commit
introduced. That is news rather than noise, and it is why the schedule exists.

## Lints are declared once, and `unsafe` is forbidden rather than denied

`#![deny(unsafe_code)]` and `#![deny(rustdoc::broken_intra_doc_links)]` were repeated
in four crate roots. They now sit in `[workspace.lints]`, inherited through
`[lints] workspace = true`. The repetition was not the problem: the crate nobody has
written yet was, since it would have been the one that forgot the attribute.

**`forbid`, not `deny`.** `deny` can be switched off again by an `#[allow]` further
down the same file. `forbid` cannot, and it is now a compile error to try. That
matters because SECURITY.md makes the no-unsafe claim about the whole workspace
rather than about whichever crate roots still carry an attribute.

**Considered and rejected: `missing_docs = "warn"`.** It is the obvious third lint
for a project whose libraries are the contract, and it produces 776 warnings on the
library targets alone, which CI's `-D warnings` would turn into a wall. That is a
documentation project, not a lint change, and it is not this one.

## `cargo install` is gated, without the lockfile

Every job passes `--locked`, which is right: the graph cargo-deny audits must be the
graph CI builds. The consequence is that nothing here resolves dependencies the way a
user does. `cargo install --path` ignores the workspace lockfile, so a semver-compatible
upstream release that does not compile would break the build-from-source path the
README documents, and the whole workflow would stay green.

A debug-profile install and a `--version` smoke test. This asks whether a fresh graph
resolves and compiles, not how fast the result runs.

## The secret scan is split: this push, and the whole history

The fixtures in this repository are transcripts of some host's output: `/etc/shadow`
lines, `pg_hba.conf` rules, connection strings. The natural way to write the next one
is to paste what a real box printed, and the natural box is the one under the desk.
gitleaks with its default ruleset gates every push and pull request over the range it
adds, which is the affordable form for a gate in front of every change.

The whole reachable history is a different question and gets a weekly workflow. It is
what makes a rule added next month apply to a commit from last year, which a
differential scan of a range that predates the rule never will.

**No allowlist, and that is a position rather than an oversight.** The 223-commit
history scans clean today, because the fixtures use values that are obviously
invented rather than real ones with a character changed. An allowlist entry is a
standing exemption for a shape, and the next real credential of that shape passes
through it silently.

**Cost:** the action is free for a public repository under a personal account and
would need a licence under an organisation, which is a thing to know before such a
move rather than from the red run after it.

## The security policy states what is not defended, redaction included

SECURITY.md exists because a tool that runs as root, reads `/etc/shadow` and emits a
complete package inventory had no channel for a vulnerability report. Its more useful
half is the out-of-scope list.

**It says plainly that redaction is not built.** The design describes values carrying
a `sensitive` annotation and a `--raw` that opts out of hashing. Collectors do set the
annotation; nothing acts on it, and `--raw` does not exist. Read quickly, the design
document promises a protection the binary does not have. The consequence, that a
stored fingerprint is sensitive operational data in full, was already recorded here;
it is now where somebody deciding where to put the file will see it.

**It names the one value that is annotated and still emitted.** Of the three collectors
that meet a credential, two keep it out structurally: `/etc/shadow`'s hash column is
dropped at parse, and a credential-bearing PostgreSQL setting reports only whether it is
set. `sysctl` marks and emits, because marking is all there is to do, so
`net.ipv4.tcp_fastopen_key` and every interface's `stable_secret` reach the document in
cleartext. Writing the policy is what turned "redaction is unbuilt" from a general
statement into two key names an operator can act on.

**Cost:** three documents now describe redaction, and they will disagree the moment one
is updated alone. The entry that lands with the redaction layer has to touch all of them.
# Reading a host without changing it

Dated 2026-09-02. Driven by a measurement on the development box: a first `rastro`
run on a freshly restored snapshot took the kernel from 68 loaded modules to 73, and
a second run added none. The five were `libcrc32c`, `nf_tables`, `nfnetlink`,
`udp_diag` and `unix_diag`.

## rastro does not change the host it describes

**The invariant: reading the host must leave it as it was found.** It was not new. It
was already stated outright, in
[the `timedatectl` reversal](#the-time-collector-reads-files-because-timedatectl-starts-a-unit):
"A fingerprint must not change the box: rastro runs as root on production to observe, and
starting a unit is a mutation however small."

**What that entry got wrong is the sentence after it**, which listed the tools believed to
be safe: "Nothing else it runs does this — `systemctl`, `ss`, `ip`, `lsblk`,
`iptables-save`, `dpkg-query` and `sshd -T` all leave the box as they found it." Two of
those seven do not. `ss` and `iptables-save` are exactly the offenders here, and they were
cleared by inspection rather than by measurement at a moment when the entry's own subject
was a tool that had been cleared the same way and was not safe.

So the invariant is now in `design.md` where a collector author meets it, rather than in a
decision entry about the time collector, and the tool list that stood beside it is
withdrawn: the five other tools were re-measured for this change and load nothing, but
"measured on this box, this kernel" is the only claim any of them supports.

Attribution was measured per command on a restored snapshot, not inferred from the
module names:

| command | modules it loaded |
| --- | --- |
| `ss -H -l -n -p -t -u` | `udp_diag` |
| `ss -H -l -n -p -x` | `unix_diag` |
| `iptables-save` | `libcrc32c`, `nf_tables`, `nfnetlink` |
| `ip6tables-save` | none, `nf_tables` being up by then |
| `ip`, `lsblk`, `systemctl`, `dpkg-query`, `sshd -T`, `pg_lsclusters` | none |

So the whole footprint was two collectors, and no other collector contributed
anything.

**Why this is worse than it looks.** It is not only that run 1 and run 2 of an
unchanged box differ, which is the symptom that surfaced it. Shared collectors run
on a pool of four over a shared cursor, and in the registry `firewall` sits at index
4 while `modules` sits at 7, so the two are dispatched in the same batch: whether
`nf_tables` appeared in run 1's *own* `modules` facet was decided by thread
scheduling. Two first runs on identically provisioned boxes could disagree. That is
gone by construction now, because nothing rastro runs loads anything.

**Recording the footprint was considered and rejected.** rastro could have kept the
richer sources and declared what it loaded, which is the same move `--staged` makes
for the binary. It fails on the thing that matters: a fingerprinter you have to
believe about its own noise is one you must audit before every diff, and the whole
value of the document is that it can be read at face value. Not causing the change
is worth more than describing it.

**Cost:** one field, and 15 ms became 105 ms on a 94-process box. Both are in the
two entries below.

**Guarded twice.** `purity.rs` fails the build if a collector source mentions `ss`,
`iptables-save` or `ip6tables-save`, which holds on any host because it reads the
source. `cli.rs` runs the real binary and asserts the module list is unchanged, which
is decisive only on a cold box and says so.

## The sockets facet is read from `/proc`, and loses the interface scope

`ss` is the canonical tool for this facet, and `ss.rs` had already argued against
`/proc/net/tcp` on two counts: it writes addresses as hexadecimal, and it "names the
holder of a socket not at all", so finding the holder means walking every
`/proc/<pid>/fd` for the inode, which is "`ss`'s job reimplemented".

The first count is true and cheap to undo. **The second was overstated**, and the
measurement is what settles it: the inode plus a readlink pass resolved 121 of 121
listening sockets on the development box, and both sources report the same 37
sockets with the same holders, states and kinds. It costs 105 ms against 7 ms, which
is nothing beside a filesystem walk, and it degrades identically to `ss -p` when
unprivileged — 2 of 90 descriptor directories readable either way.

**What is genuinely lost is `SO_BINDTODEVICE`**, which `ss` prints as
`127.0.0.53%lo` and the kernel returns over diag netlink and nowhere else. No column
of `/proc/net/tcp` carries it. **It is not recoverable by inference**, which was
checked rather than assumed: `127.0.0.53` carries the scope and `127.0.0.54` does
not, and neither appears in `ip addr`, so deriving the scope from the address would
invent one for the second. The field is therefore removed rather than kept always
null, because a key that is always null asserts rastro looked.

The residue is a wildcard bind that is really reachable on one interface only, which
now reads as globally exposed. Three of 37 sockets on the development box carried a
scope and none of them was that shape.

**`*` is gone from the address vocabulary too**, and this one costs nothing. `ss`
prints `*` for a dual-stack socket and `[::]` for an IPv6-only one, a distinction it
draws from a socket option `/proc` does not publish. The arrangement is still
readable from the facet — a dual-stack socket appears once as an IPv6 wildcard, a
family-separated pair appears as two rows — so only the spelling of one row changed.

**The collector's version went to `2`.** On identical host state the facet now omits
a key and spells one wildcard differently, so a consumer diffing across the change
has to be able to see that the collector moved rather than the host.

## A firewall backend is read only where its subsystem is already resident

`iptables-save` is an alternatives symlink, and on Debian 12 it points at
`iptables-nft`. Running it opens an nfnetlink socket and the kernel loads
`nf_tables`, which pulls `nfnetlink` and `libcrc32c` behind it. Debian also ships the
implementations under their own names, and those are what rastro runs now:
`iptables-legacy-save`, `iptables-nft-save` and the two IPv6 twins. Four backends
rather than two, because legacy and nftables are separate rulesets that can hold
tables at the same time and each tool reports only its own.

**Each backend declares the kernel subsystem it would provoke, and is read only when
that subsystem is already there.** Asking a resident subsystem loads nothing, which
was measured both ways: with `ip_tables` up, `iptables-legacy-save` loaded nothing;
with `nf_tables` up, `iptables-nft-save` loaded nothing.

**A subsystem the kernel has not loaded holds no ruleset**, so its absence is an
observation rather than a silence, and the facet is now *more* informative than it
was. Residency is read from `/proc/modules`, and from `CONFIG_*=y` in
`/boot/config-<release>` for a kernel built with the subsystem compiled in.

**The dangerous case is a source rastro cannot read.** With no
`/boot/config-<release>`, in a container or with `/boot` unmounted, an unloaded
subsystem might still be compiled in and holding rules. With no readable
`/proc/modules`, rastro knows nothing about what is loaded at all. Either way
residency answers `undetermined` and the backend reports `error` rather than `absent`,
so `absent` is given only when both sources were read. Reporting a filtered box as an
unfiltered one is the one failure this facet must not have.

So each backend now carries a status instead of a ruleset or `null`:

- `ok` — the tool ran; `tables` may still be empty, meaning the box filters nothing.
- `absent` — the subsystem is not loaded, so no ruleset exists. `tables` is `{}`.
- `error` — the subsystem is resident and the tool is missing, or residency could not
  be told. `tables` is `null` and `reason` says which.

That distinction is the reason the shape changed rather than a bonus: before this,
an empty dump and a subsystem nobody had loaded produced the same empty object, and
the gate would have made "no rules" ambiguous without it.

**Still not covered:** a ruleset written natively with `nft`. These four dump what was
written *through the iptables interface*. What is new is that an unloaded `nf_tables`
is now a positive statement about the native ruleset too, since nothing can be
holding rules in a subsystem the kernel has not loaded.

**Unverified, and worth naming:** that nftables rules cannot exist while `nf_tables`
is unloaded. The refcount evidence is consistent — `nf_tables refcount=0` with no
rules, `nfnetlink refcount=1 used_by=nf_tables` — but that is not the converse, and
proving it means adding a rule to a box, which is a change rather than a reading.

**The collector's version went to `2`**, and the key set changed from two names to
four, so a diff across the change is unmistakable.

# Seeing a password change without holding a password

Dated 2026-09-02. Driven by a test-VM run either side of a provisioning script:
three role passwords were rotated, both fingerprints were byte-identical, and the
document said `password_method: scram-sha-256` on both sides.

## A role password change is visible, and is hashed twice to get there

`Role` carries `password_digest` beside `password_method`. The query prints
`encode(sha256(convert_to(a.rolpassword, 'UTF8')), 'hex')` and rastro records an
[`Xxh3Digest`](#one-digest-spelling-lives-in-the-port) of that hex. PostgreSQL re-salts
on every `ALTER ROLE ... PASSWORD`, so the stored verifier differs even where the new
password equals the old one, and a rotation that was invisible is now a changed line.

**Why the method alone was not enough.** `password_method` names the algorithm, so a
rotation within one algorithm left it untouched. It would only move on a switch between
schemes, say scram to md5, which is drift worth seeing and is not what a rotation is.
Anybody reading a `postgresql` diff as evidence that credentials were untouched was
reading it wrong, and nothing in the document said so.

**The first hash is on the server, so nothing to leak ever arrives.** The verifier is
never read into this process: not into the psql pipe, not into a parser, not into a
model field, not into rastro's heap. Selecting the verifier and hashing it here is one
line shorter and trades a structural guarantee for a disciplinary one. Absence of the
material is a promise no later mistake in the parser or the renderer can weaken, which
is the argument `Setting` already makes when it withholds a credential-bearing value
instead of trusting an annotation.

**The second hash is so a pile of leaked documents is not a dictionary.** What the
document carries is not the server's sha256 but a digest of it, at 64 bits. So it is no
standard digest of any standard input, and cannot be looked up in or built into a
precomputed table. The one inference it leaves: two hosts printing one digest for one
role share a verifier, hence a password *and* its salt, which happens only where a
pre-computed verifier was pushed to both.

**Only a SCRAM verifier is digested, and that condition is doing the security work.**
Neither added hash makes a verifier un-guessable; the *salt inside the verifier* does.
SCRAM has a random one per `ALTER ROLE`, and the digest keeps neither it nor the
iteration count, so a candidate password cannot be tested against a fingerprint. An md5
verifier is `md5(password || rolname)` and has no random salt at all: its only variable
input is the role name, which is this facet's own key, sitting in the same document. A
digest of it is therefore a fast offline oracle — md5, sha256 and XXH3 over each
candidate, compared against 64 bits — and unkeyed hashing cannot fix that, because
everything the attacker needs to recompute it is published beside it. So the query
digests SCRAM and prints an empty column for anything else.

**Fail closed, by testing for SCRAM rather than excluding md5.** A scheme a later
PostgreSQL adds gets no digest until somebody has checked how it is salted, rather than
one by default. The cost is that an md5 role's rotation stays invisible, which is the
state this entry set out to fix; it is the right trade because `password_method: md5` is
already reported and is itself the finding, and because a keyed construction is the only
alternative and would need a persisted secret that a stateless generate-only tool has
nowhere to keep.

**An absent digest means two things, and the field beside it says which.**
`password_digest: null` is either a role with no password or a verifier that must not be
digested. `password_method` distinguishes them — `null` against `md5` — which is what
that field is for, so no sentinel is invented in the digest field to repeat it.

**Nothing here is marked `sensitive`, and that is not an oversight.** The verifier is
sensitive and never reaches a value to annotate; the digest is not, because
[`Sensitivity::Sensitive`](../crates/rastro-fingerprint/src/observation/annotation.rs)
means *must not be printed as it stands*, which is false for a digest and would tell the
redaction layer to suppress the one value that makes a rotation visible.

**Revisit when redaction lands.** The designed mechanism —
secrets hashed at serialisation time, `--raw` opting out, as
[the security policy states plainly](#the-security-policy-states-what-is-not-defended-redaction-included) — would
have the collector select `rolpassword`, mark it `sensitive`, and let the renderer decide. That
is the better shape and it is unreachable today, because nothing acts on the annotation and
`--raw` was not built, so marking a verifier sensitive would have printed a verifier. Both now
exist, and this remains the one value where `--raw` cannot be a render-time decision: the
material is not in the process to render, so opting out means the collector asking a different
question of the server. That is a query change, not an annotation, and it is still undecided —
see [`--raw`, and a document that admits which one it is](#--raw-and-a-document-that-admits-which-one-it-is),
which records that `--raw` does not claim to cover this facet.

**The collector's version went to `3`.** On identical host state every role now carries
a key it did not, so a consumer diffing across the change has to be able to see that the
collector moved rather than the host.

**Cost: the roles query now needs PostgreSQL 11.** `sha256` arrived in 11 and this query
has no fallback, so on 10 or older the roles read fails loudly and the whole roles list
is lost rather than only the digest. The hba read already needs 10, Debian 10 shipped
PostgreSQL 11, and 11 has been end-of-life since 2023. A version-gated `md5(rolpassword)`
for older clusters was considered and rejected: it would make one password produce two
different digests across a major upgrade, and report every password as changed on the run
after one.

**`convert_to` rather than `rolpassword::bytea`.** Casting text to bytea runs the value
through bytea's input parser, which interprets a backslash. No base64 or hex verifier
contains one, so the cast would work and would be wrong for a reason a reader could not
see.

## One digest spelling lives in the port

`Xxh3Digest` is in `rastro-collector`, beside the rest of the shared collector
vocabulary, and both the walker's entry digest and the role password digest are it.

**Why not one per collector.** They were, for one commit: `MetadataDigest` in the
filesystem collector and a `PasswordDigest` beside it in postgresql, same algorithm and
same width by hand. That is the exact failure the port's `value_objects` module exists to
prevent — two collectors spelling one concept differently in a single document — and the
second one is where it stops being hypothetical. An out-of-tree collector reaches it
through `rastro_collector::` like every other shared type, so it does not have to invent
a third.

**What each collector kept.** The digest is generic; what is hashed is not. `CanonicalBytes`
stays in the filesystem collector, because length-prefixing an entry's attributes so two
paths cannot collide by construction is a fact about walked entries. The 64-hex column
check stays in the postgresql source layer beside its sibling column parsers, because
`encode` printing lowercase is a fact about that query.

# The extended-verification label opens the deep gates

Dated 2026-09-03. Supersedes the naming in
[the container gate](#the-container-gate-is-a-workflow-of-its-own-opened-by-a-label),
whose substance stands.

`extended-verification` on a pull request runs every workflow in the deep tier. Today
that is `distributions.yml`: Debian and Alpine, root and unprivileged. `CONTRIBUTING.md`
holds the criterion for applying it.

**One label for the tier, not one per workflow**, so a contributor never has to work out
which deep check their change needs. That is a reviewer's call.

**The label names what is promised; the workflow names what it does.** `container` and
`full-matrix` both named today's implementation, which the next tenant falsifies.

**The label must exist in repository settings.** Gated on one that does not, a workflow
never runs and reports nothing.

# Redacting a sensitive value

Dated 2026-09-03. `Sensitivity` had been carried on every node since the model was
written and nothing acted on it.

A value a collector marked sensitive renders as `redacted:sha256+xxh3:<digest>`: sha256
of the material as lowercase hex, then XXH3-64 over those hex characters. Sensitivity
descends into a subtree, as volatility does. A null is not redacted.

**Redaction is not a view.** A volatile value is dropped from the diffable view and kept
in the complete one; a sensitive value is withheld from both. So `Presentation` carries a
`Disclosure` beside the `View`, and `From<View>` fills in `Redacted` — redaction-by-default
is then structural, not a discipline.

**Two stages, two jobs.** sha256 makes the stand-in defensible for a secret; XXH3-64 keeps
every digest in the document one width and one spelling. The pair also reproduces the
PostgreSQL role digest exactly, so archived fingerprints still compare — true only while
the hex is lowercase and the XXH3 covers the hex characters, which `tests/redaction.rs`
holds with an outside vector.

**Non-text scalars carry their type; text does not.** Untagged, boolean `true` and text
`"true"` digest alike and a type change reads as unchanged. Text is untagged because
comparability requires it.

**A digest proves change; it does not hide a guessable value** — see `SECURITY.md`.

**Supersedes [One digest spelling lives in the port](#one-digest-spelling-lives-in-the-port).**
The renderer spells the same digest, and `rastro-collector` depends on `rastro-fingerprint`,
so `Xxh3Digest` moved into the document crate. The port re-exports it.

**Not built:** `--raw`. Until it exists no sensitive value can be read out of a document.
The two collectors that withhold a credential structurally still do, and reversing either
means a new entry.
# nginx: the configuration as it lies, not as a test run reports it

Dated 2026-09-03. The first Layer 3 collector for a service with no runtime
introspection at all, which turned out to be the interesting part.

## `nginx -T` is not a read, and it is not effective state either

The plan said `nginx -T`. Two things are wrong with it, and the second is worse
than the first.

**It changes the host.** Testing a configuration loads it, and loading it opens
every log file it names — creating the ones that are not there. Measured on
nginx 1.30: a configuration naming `/tmp/logs/created-by-config-test.log`, which
did not exist, left a root-owned empty file behind after a plain `nginx -t`. `-T`
is `-t` plus a dump, so it does the same. A fingerprint tool that creates files
on the box it was called to describe has changed the thing it is measuring, and
the run after it differs from the run before it for no reason but rastro. That is
the `modules` autoload defect again, in a facet that had not been written yet.

**And it would not have bought effective state anyway.** `sshd -T`, `systemctl
show` and `pg_settings` each report what the *running* service is using. `nginx
-T` re-reads the same files from disk and re-resolves the same includes: a vhost
edited without a reload reaches `-T` while the running server carries on with the
old one. Outside the commercial API, nginx has no runtime introspection to ask.
So the choice was never effective-state-versus-files; it was files read by nginx
against files read by rastro.

What `-T` genuinely offers over reading the files is include resolution — it does
not flatten includes, apply inheritance or resolve variables, so a parser is
needed either way.

**The rule this establishes, which is narrower than "rastro may parse configs".**
Parse a service's configuration only where the service offers no non-mutating way
to report its own effective state, with the measurement attached. Where it does,
ask it. Apache, haproxy and docker each need their own measurement before they
come through this gate; `sshd -T`, `systemctl show`, `sysctl` and `psql` are all
unaffected.

This also **corrects an earlier entry**. "The general rule this does not overturn"
above claims `nginx -T` and `sshd -T` do not change the host. It is right about
`sshd -T` and wrong about `nginx -T`, and the correction is here rather than in
an edit to that entry.

**Cost, and what pays it down.** rastro's include resolution can in principle
disagree with nginx's. Fixture tests pin what rastro does with each rule, but a
test whose expected file list was written by the same person who wrote the resolver
only re-encodes that person's belief, and cannot catch a belief that was wrong to
begin with.

**So one test asks nginx.** `nginx -T` prints a `# configuration file <path>:`
marker for every file it read, which makes it the one non-circular oracle for the
question, and `tests/nginx_conformance.rs` compares that list against rastro's over
a tree holding every awkward case at once: a relative glob, an absolute include, a
nested include, a symlinked one, a glob matching nothing, and a file pulled in from
two places.

**The boundary is the point, and it holds.** The shipped binary never runs `-T`,
because testing a configuration creates every log file it names. The test may,
because there the mutation is contained twice over: the run is given a prefix inside
the test's own scratch tree, and `-e stderr` leaves it no error log to create. It
needs no privilege, so it runs in both halves of the container suite. nginx is
installed for it in `scripts/container-suite.sh` and in the CI check job, and
nowhere else.

**It earned its place on the first run**, by catching a divergence no fixture test
would have: a file included from two places was being recorded twice, where nginx's
own dump lists it once. See the entry on that below.

## The grammar is measured rather than remembered

Two rules would have been wrong from memory, and both are the kind that corrupt a
value silently rather than failing loudly.

**Escapes work outside quotes too.** `a\tb` in a bare token holds a tab, and
`a\;b` is one token whose semicolon terminates nothing, because nginx spends the
backslash before it looks for a delimiter. A grammar that ended the token at that
`;` would report two directives where nginx reads one.

**`${name}` is one token, and a bare `{` is not.** Two measurements, and the pair is why
this is a special case rather than "braces are ordinary characters":
`access_log /tmp/literal{x}.log;` is refused by nginx itself with "directive access_log is
not terminated by \";\"", while `proxy_pass http://${backend};` gets past tokenising and
fails on the *name* with "unknown backend variable". So nginx suspends the delimiters
inside `${…}` and enforces them everywhere else. A grammar that let that `{` open a block
would refuse the file, and a configuration using the braced form — which is ordinary —
would produce no facet at all.

**An unrecognised escape keeps its backslash.** `\q` is `\q`, so dropping the
backslash would put a value in the document that was never in the file. The six
that are spent are `\"`, `\'`, `\\`, `\n`, `\r` and `\t`, in both quote styles and
outside them.

Also measured: `#` starts a comment only where a token starts, so `a#b` is one
token; and a quote opens a quoted token only where a token starts, so `a"b` is
one token as well.

**How.** Because `nginx -t` creates the log files a configuration names, a quoted
`access_log` path becomes a filename on disk, and `od -c` on that name shows
exactly what nginx's parser made of the token. The defect above is what made the
measurement possible.

## Includes resolve like `glob(3)`, sorted by bytes

Measured: a relative include resolves against the prefix (`/usr/share/nginx` on
Debian, not `/etc/nginx`), a glob is read in sorted order, a glob matching nothing
is not an error, and a literal include of a missing file stops nginx from
starting — so that one is recorded as a refusal rather than passed over.

**Byte order rather than the caller's collation.** `glob(3)` sorts with
`strcoll`, so the same directory can order differently under two locales. rastro
sorts by bytes so a fingerprint means the same on every box; the two differ only
where two files differ solely in case or punctuation *and* both set the same
directive.

**A bracket expression is refused, not guessed.** `include conf.d/[a-m]*.conf`
would need `glob(3)`'s character classes; matching it wrongly would report a set
of vhosts the server does not have, and nothing in the document would say so.

**An include cycle stops.** nginx recurses until it fails; rastro records the
second visit as a refusal and carries on. Cycle identity is the path resolved
through symlinks, because `sites-enabled/x` and `sites-available/x` are one file
under two names.

## nginx has two bases for a relative path, and they differ on Debian

Found by running the collector against a real Debian nginx rather than by reading
about it. `prefix` is `-p`, or `--prefix` at build time, and a cache or a temp
path resolves against it. `conf_prefix` is the **directory of the configuration
file** — `-c`'s, or `--conf-path`'s — and an `include`, a certificate and a user
file resolve against that. Debian builds with `--prefix=/usr/share/nginx` and
`--conf-path=/etc/nginx/nginx.conf`, so the two are different directories.

The first implementation used the prefix for everything, which on Debian looks
for every relative include in a directory that holds none. Measured on nginx 1.30
started as `-p /tmp/altprefix -c /etc/nginx/nginx.conf`: a request against a
location with `auth_basic_user_file relative.htpasswd` logged
`open() "/etc/nginx/relative.htpasswd" failed` — the configuration's directory,
neither the prefix nor the `-p`.

nginx derives `conf_prefix` by taking the directory of whichever configuration
file it ended up with, so rastro does the same, and the facet records both bases.

## A file included twice is read twice and recorded once

Found by the conformance test above on its first run, which is the kind of thing it
exists for: rastro listed a twice-included file twice, and `nginx -T` lists it once.

Both halves were then measured on nginx 1.26. The dump prints one marker for the
file, and including a `server` block twice produces the `conflicting server name`
warning — which only two server blocks can produce. So nginx reads the file twice
and *reports* it once, and rastro now does the same: `directives` holds its contents
twice, because that is what the server is running, and `files` names it once,
because that list answers which files make up the configuration and a second
identical entry with an identical digest answers nothing.

## A file's digest is over its parsed form, not its bytes

A comment added, a block re-indented or an argument requoted leaves nginx serving
exactly what it served before. A digest of the bytes would report all three as a
change to the service, which is the noise this tool exists to remove. The digest
is taken over the directives with every token length-prefixed, so no separator can
be forged by a value containing it.

**Cost:** the digest covers the whole file, so a change to a directive the model
*does* name shows in both places. The sharper alternative — digesting only what
the model does not cover — was rejected for now because it couples every file's
digest to the model's coverage, and extending the model would then move digests
for files nobody touched.

## Order is kept where nginx reads it, sorted where it does not

Virtual hosts, locations, access rules and certificate/key pairs keep their
written order, because nginx uses it: a default server is resolved by it,
locations are matched in it, `allow`/`deny` is first-match, and the first key
belongs to the first certificate. Server names, listen addresses, listen options,
pool members and pool member parameters are sorted, because nginx reads each as a
set and an operator rearranging one has changed nothing.

## The workers date the last reload, and the master cannot

The obvious signal for "is the configuration on disk the one being served" is the
master's start time against the newest configuration mtime. It is wrong, and
measurably: a reload leaves the master untouched — its `/proc/<pid>` mtime did not
move across a `nginx -s reload` — while every worker is replaced. So the *oldest
worker's* start time dates the last reload, and that is what the facet records
beside `configuration.newest_modified`.

**Workers are matched to their master by parent, not by title.** A box running two nginx
instances has two sets of workers, and a binary upgrade briefly has the old master's
running beside the new one's. Counting every process titled `nginx: worker process`
against one master would make its worker count wrong and its oldest-worker time — the
thing that dates the reload — belong to somebody else's reload. The parent comes from
the `PPid:` line of each worker's `status`.

**A process that leaves mid-scan is skipped, not fatal.** During a reload every worker is
replaced, so one listed a moment ago can be gone before its start time is read. Failing
the facet for that would repeat, in a new collector, the mistake this same branch fixes in
the `processes` one. A master that goes the same way is reported as no master, which is
what it is.

**A start time from a directory's mtime.** `/proc/<pid>`'s mtime is the moment the
process began, measured against a freshly started master. Reading it that way
needs no clock-tick arithmetic, no `btime` and no `sysconf` — which matters,
because this workspace forbids unsafe code and `sysconf` is a call rather than a
constant.

**The master's own command line is where `-c` and `-p` are read back.** nginx
rewrites its argument vector into a process title, so `/proc/<pid>/cmdline` reads
`nginx: master process /usr/sbin/nginx -c /etc/nginx/other.conf`. rastro reads the
configuration the running server was told to read, and the facet says which
authority decided the path. The title has lost its quoting, exactly as
`systemctl show` has lost a unit's, so a path holding a space cannot be recovered
from it.

**` (deleted)` is allowed for when matching the binary.** After a package upgrade
the kernel marks `/proc/<pid>/exe` that way; comparing without stripping it would
report an upgraded-but-not-restarted server as no server at all. The marker itself
is recorded, because it is the state.

## A basic-auth password is digested only where its verifier is salted

The same rule as the postgresql facet's role passwords, the same reason, and — since
the branch was rebased onto the redaction work — the same recipe: XXH3 over the
lowercase sha256 hex of the verifier. postgresql digests a role's password that way
with the server computing the sha256, and the renderer's own redaction takes the
same two stages in the same order, so one document holds one function of "a
withheld password" rather than three.
`$apr1$`, `$2y$` and `$5$`/`$6$` carry a random salt, so a digest of one says only
that the password changed. `{SHA}` is an unsalted SHA-1 of the password itself:
anybody holding the document could hash a guess, spell it the way the file does,
digest that and compare, which would turn a fingerprint into an offline oracle
over everybody in the file. Recognising the salted schemes rather than excluding
the unsalted ones fails closed, so a scheme nobody has checked yet gets no digest.

## The certificate is read; the key is only described

A certificate is the public half — every client that connects is handed a copy —
so reading it puts nothing in the document the server does not already give away.
It is also the difference between a renewal and an edit: a path and an mtime say a
file changed, while a serial, a validity window and a digest say whether the same
authority reissued the same names.

The private key is `stat`ed and never opened. No digest of it either: that would
be a way to confirm a guessed key, and there is nothing a fingerprint could do
with it worth that. What is recorded is the mode and the owner, which is what
catches a key that became group-readable.

**Through the symlink, and that is not the usual call.** A key is very often reached
by one — a Let's Encrypt deployment points `ssl_certificate_key` at
`live/<host>/privkey.pem`, itself a link into `archive/` — and a symlink's own mode
is `0777` on Linux. Describing the link would report every such key as world-readable
and every genuinely world-readable one as fine, which is worse than not recording the
mode at all. nginx opens the target, so the target answers the question. The link
itself is the `filesystem` facet's business, and the path recorded here is still the
one the configuration named.

**The serial is the number, not the bytes.** DER pads a serial whose high bit is
set with a leading zero byte, so the raw form and the one `openssl x509 -serial`
prints differ for half of all certificates. The number is the same either way.

**No "days remaining".** It would differ between two runs of an unchanged host,
which is the one thing a value in this document may not do. The expiry is an
instant; how close it is is the reader's arithmetic.

## The trees nginx writes into are sealed

`proxy_cache_path` on a busy server is tens of thousands of files that nginx
creates, renames and unlinks on its own schedule. Walking them reports change on
every run for reasons nobody caused, so the claim is `sealed`, as it is for a
PostgreSQL cluster's data directory: the root entry stays, nothing under it is
walked, and the `invocation` facet names this facet as the reason.

Both halves are claimed — the trees the configuration names, found by the
`_cache_path`/`_temp_path` suffix rather than by a list of directive names, and the
five temp trees of the binary itself.

**All five of those, always, and resolved against the prefix.** A build that named
none of them on its configure line still writes into all five, at nginx's own
defaults under the prefix, so reading only the arguments would leave them unclaimed
on exactly the hosts nobody packaged. A configure argument may also be relative, and
a relative path is one `WalkedTree` refuses — it would have been dropped from the
claims without a word, and the walk would have hashed a cache.

**Cost:** claims are gathered before any collector runs, so the configuration is
read twice per run. A configuration edited between the two readings is claimed as
it was and reported as it became, which is the narrower of the two wrong answers.

## `http` and `stream` are separate services in the document

A box can proxy a database port and serve a website, and the same port number in
the two contexts means different things. They are separate nodes, so a server
moved from one to the other — a change to what happens on every connection —
reads as the change it is rather than as nothing at all.

A `stream` server is a different shape rather than a poorer one: no
`server_name`, no locations, because nginx has no request to name a host with.
What it does share with a virtual host is where it listens, what it serves TLS
with, who may reach it and where it logs, so those are the same types and compare
directly. An `upstream` is spelled identically in both contexts, so it is one
model in both.

## Log destinations are state

A request log redirected to `off`, to a syslog server, or to a path nothing
rotates changes what a box can tell you about itself afterwards, and none of it
touches a byte of served content — so nothing else in a fingerprint would say it
moved. Each `access_log` and `error_log` is recorded where it is declared, with
the destination as written (a path, `off`, or `syslog:…`) and the rest of the
directive kept whole as detail: a format name for one, a level for the other.

Sorted rather than kept in written order, because a block may declare several and
nginx writes to all of them.

## Inheritance is not resolved, and that is not a gap

rastro reports the state of a server. What nginx would *make* of that state is a
different question, answered by nginx.

Resolving inheritance means reimplementing nginx's merge semantics from the
outside, and there is no single rule to reimplement: simple values inherit unless
redefined, array-valued directives like `add_header` and `allow`/`deny` are
replaced wholesale the moment an inner block declares one, `auth_basic off` is a
sentinel rather than a value, and every third-party module writes its own merge
function. Getting any of it wrong means asserting an effective value the server is
not using, which is the failure this tool exists to prevent — and getting it right
would make rastro a configuration evaluator, which is not what it is.

So each block's own declarations are recorded at the level that declares them,
and the reader does the merging with nginx's rules rather than with rastro's
imitation of them.

The same reasoning rules out the syntax check. `nginx -t` would answer "is this
configuration valid", which is nginx's question at reload time; a file the grammar
cannot read is already recorded as a refusal, which is the part that is state.

## What this facet does not model

Named because a silent gap is worse than a stated one.

- **Variables.** `proxy_pass http://$backend` is recorded as written. Resolving it
  would mean claiming to know a value that only exists per request.
- **The `http` and `stream` blocks' own directives**, which are declared outside
  any server and reach the document only through their file's digest.
- **Directives outside the model**, which is most of them. Each is covered by its
  file's digest, so a change to one is visible even where its meaning is not.

## A dying process answers `ESRCH`, and the guard only knew `ENOENT`

Not an nginx decision, but this work is what surfaced it: installing nginx into
`scripts/container-suite.sh` for the conformance check above made the container's
process table busy enough to lose a race that had always been there.

The `processes` collector already knows that listing `/proc` and then reading each
entry is inherently racy, and drops a process whose files have gone. Its guard
tested [`ErrorKind::NotFound`](std::io::ErrorKind) alone, under a comment claiming
that `ESRCH` arrives that way too. It does not: Rust leaves `ESRCH` uncategorised,
so a process reaped *while* its `status` was being read fell through to the failure
arm and turned the whole facet into an `error`. The determinism harness caught it,
naming the facet and the two states — `could not read /proc/9214/status: No such
process (os error 3)` against a clean second run.

On a production box, which is where rastro runs, a process exiting mid-walk is not
an event: it is Tuesday. The facet would have failed there far more often than in
CI, for no reason at all.

The guard now names both errnos, and the distinction is reachable from a test,
because neither can be provoked from a fixture and the alternative is a rule
nothing checks.

---

# Containers: one facet, several engines

Dated 2026-09-08. The third Layer 3 collector, and the first for a subsystem that
comes in more than one implementation of the same idea.

## docker and containerd report themselves without changing the host

The nginx gate above says a service's configuration may be parsed only where the
service offers no non-mutating account of its own effective state, **with the
measurement attached**. It names docker as one of the three that had to be measured
before coming through. Here is that measurement.

On a quiet Linux running docker 29.8.0 with containerd 2.3.4 underneath it, a full
`stat` inventory of `/var/lib/docker`, `/var/lib/containerd`, `/run/docker`,
`/run/containerd` and root's home was taken, then the reads were run, then the
inventory was taken again:

`docker ps`, `docker inspect`, `docker info`, `docker image inspect`,
`docker volume inspect`, `docker network inspect`, `ctr containers info`.

**Zero changed entries**, against a control interval that proved the box was
otherwise still. Nothing was created in the working directory either. So there is no
configuration for rastro to parse here: the engines answer for their own effective
state, which is what the design prefers wherever it is available.

**A second property comes free from the execution seam.** It clears the environment,
so no `DOCKER_HOST` and no client context can point the read at a daemon on another
box. The facet is about the box rastro is running on, structurally rather than by
convention.

**What is still owed:** podman. It is daemonless, and a read command initialises a
store rather than asking one, which is exactly the shape of the nginx defect. It does
not come through this gate until it has its own measurement on a quiet box.

## One `containers` facet, keyed by engine flavour

Not one facet per engine. `packages` already covers dpkg and apk together, for the
reason that also applies here: two collectors claiming one facet name would fail the
run, and an operator asking about containers is asking one question.

**Two engines legitimately sit side by side, and both are reported.** docker runs
containerd underneath itself, so a docker box has both, describing the same
containers at two different levels. Suppressing containerd's `moby` namespace because
docker is present would be rastro deciding which of two true accounts an operator is
allowed to see. Keeping them apart is what lets them disagree, the same reasoning
that keeps an exporter's configured endpoint separate from what `sockets` observed
bound.

**A shared identity core, per-engine detail.** containerd's container is not docker's
spelled differently: it has no published ports and no restart policy, because those
are docker's abstractions above it. A single container type would either lie by
omission or grow an optional field for every concept any one engine has. So the value
objects are shared — the id, the name, the image reference, the digest — and each
engine contributes the detail its own concepts support.

**This corrects the wording of an earlier entry.** "v1 collectors: Layers 1 and 2,
plus three Layer 3 starters" names the third starter `docker`. The facet is
`containers`, and docker is the first engine in it.

## `docker version` is the probe, because it is the only read that survives a dead daemon

Measured, and it decides the whole detection ladder. On a box where docker is
installed and nothing is answering on the socket, docker 29.8.0 answers:

| read | exit | stdout |
| --- | --- | --- |
| `docker version --format '{{json .}}'` | **0** | the client half, with `"Server": null` |
| `docker info --format '{{json .}}'` | 1 | a skeleton of empty fields |
| `docker ps` | 1 | nothing |

All three write the connection failure to stderr. The execution seam refuses a
non-zero exit's output entirely, so `info` and `ps` cannot tell "no daemon" from "a
broken read" — both arrive as the same failure. `version` can, and it is one of the
tools that answers on the wrong stream, which is what `run_capturing_stderr` exists
for.

**The exit code is not a contract across versions, and that was measured too.**
Debian 13's docker, 26.1.5, exits **1** for the same read against a socket that is
not there, printing the same `"Server": null` document to stdout it will not be
credited for. So on that client an engine whose daemon is down reaches the document
as a facet `error` carrying docker's own complaint, rather than as the observed state
of an installed engine with nothing answering.

**Accepted rather than worked around.** The alternatives are worse: reading stdout
from a non-zero exit means the seam no longer refuses partial output, which is a
hardening rule that exists because a truncated answer treated as an answer is the bug
that disqualified configsnap. Matching docker's error text for "permission denied"
against "is the daemon running" is a version-dependent guess dressed as a fact. And
inferring "not running" from a missing socket at the default path is wrong for any
dockerd started with `-H`.

An `error` naming the reason is not a lie about the box — it says rastro could not
establish the state, and why. The distinction is kept where the client offers it, and
the same shape serves podman, which is daemonless and answers for itself.

**One case is a failure on every version, and should be:** an unprivileged run. The
socket is there, the daemon is answering, and the caller may not use it. Measured on
26.1.5 as a non-root user, `version` exits 1 with `permission denied while trying to
connect`, and the facet is an `error`. Recording that as "unreachable" would be a
false statement about a daemon that is running perfectly well.

It pays for itself twice over: the same document carries the client version beside
the server's, which differ on a box whose package update has restarted neither, and
the components — which containerd, which runc, which init the engine actually runs. A
runc replaced under a running docker is exactly the change a fingerprint is taken
around, and it is invisible in the engine's own version.

## An engine installed with nothing answering is state

Three facts, kept apart:

- no engine rastro can read: the facet is `absent`;
- an engine installed with nothing answering: `ok`, with `daemon` `unreachable`, the
  reason the client gave, and **no server node at all**;
- rastro unable to look: an `error`, loudly.

The server node's absence is structural rather than a convention. A box whose daemon
did not answer has nothing to say about its storage driver or its containers, so
there is no field for a reader to mistake for "asked and told nothing" — the same
reason the postgresql facet keeps a cluster's configured half apart from its observed
one.

**Cost, accepted knowingly:** `absent` means "no engine rastro knows of". A box
running LXC or incus reads as absent, which is a limit of rastro rather than a fact
about the box. The alternative, an unconditional `present`, would put an
engine-shaped empty answer into every fingerprint of every box that has never run a
container.
## Keyed by container name, not by container id

An id is minted afresh every time a container is created. Keyed by id, a
`docker compose up` on an unchanged definition would report every container as
removed and a new one added, which is a diff that says nothing.

A name survives recreation: compose derives it from the project and the service, and
an operator who names nothing still gets a name that is stable until they recreate
the container themselves. The id is recorded as a value, where a reader sees it
change and knows the container was rebuilt.

## A container that will delete itself is volatile whole

`--rm` declares a job rather than a tenant. A cron-driven `docker run --rm` exists
for a few seconds, so two runs of a box nobody touched legitimately disagree about
whether it is there.

The `processes` facet met the general form of this and had to annotate its whole
table volatile, because a process table cannot be byte-identical on a machine that is
doing anything. A container list is not like that: a container is declared, it
outlives the run, and whether it is up is the first line an operator reads. So the
entries stay and the exceptions are annotated — which is what `Volatility` is for,
rather than something the byte-identity contract had to be weakened to accommodate.

**Keyed on the engine's own record of the intent**, `HostConfig.AutoRemove`, and not
guessed from a name or an uptime. The container is still reported in full in the
complete view, where somebody standing in front of the box can see what ran.

The moving values inside a container that stays get the same treatment one at a time:
both stamps and the restart count are volatile, because a container restarting under
its policy moves all three with nobody having touched the box. The status is not, and
deliberately: `running` becoming `exited` is the single most useful line in a diff of
a container host.

## A container that vanished while being read is recorded, not dropped

Reading a box's containers takes two steps, the id list and then one read per
container, and a `docker run --rm` from cron can end between them.

**One read per container rather than one read for all of them**, which is the
decision the race forces. `docker inspect` given several ids exits non-zero if any
one of them has gone, and the seam refuses a non-zero exit's output entirely, so a
single ephemeral container ending mid-run would cost the whole facet every other
container on the box. Read one at a time, that loss is one entry in
`unreadable_containers`, carrying the id and the engine's own complaint.

The list is volatile, for the same reason the ephemeral container itself is: a
container that comes and goes on its own is the host changing on its own. The cost is
one subprocess per container, which is what running the collectors concurrently is
for.

## The manifest digest is not there below docker 29

`docker inspect` on 29.8.0 carries an `ImageManifestDescriptor`, whose digest is what
a registry would serve for the container's image. Debian 13's docker, 26.1.5, does
not have the field at all — measured on both.

So the container's image is recorded as three values rather than one: the reference
the operator wrote, the id docker resolved it to, and the manifest digest **where the
engine offers it**. The id is the strongest of the three anyway, being a digest over
the image's configuration, and the repo digest reaches the document through the image
list rather than through every container that runs it.

## Every environment value is sensitive, and none of them is judged by name

The `sysctl` facet decides sensitivity from the key, because the parameters holding
a secret are a closed set somebody can enumerate. A container's environment is the
opposite kind of thing: it is whatever the operator put there, and the name is a poor
witness in both directions.

`DSN=postgres://app:s3cret@db:5432/app` carries a credential and matches no keyword a
rule could look for. `MYSQL_ROOT_PASSWORD` announces itself. A rule that guesses fails
in the direction that leaks, so there is no rule: every value is withheld and reaches
the document as a digest, in both views.

The names stay public, which is what makes the facet useful. A diff says `PGPASSWORD`
changed, and the value that says so is not the password — the same shape the postgresql
facet uses to make a role's password rotation visible without holding the password.

**Cost, accepted knowingly:** `PATH` and the rest of an image's benign environment are
digested too, so the complete view reads less well than it could. `--raw` is where that
is paid back, once it exists.

Labels are the asymmetry, and deliberately: a label is metadata somebody attached to
describe the container, and for a container nobody named by hand it is the only durable
link back to the definition it came from, since compose writes its project, its service
and a hash of the config it rendered. Those are recorded as they stand.

## A tmpfs mount is in neither list the others are in

Measured on docker 26.1.5. A container started with `--tmpfs /scratch:rw,size=64m`
reports **no `Mounts` entry at all** for it. The only place it appears is
`HostConfig.Tmpfs`, as a destination mapped to its raw option string.

So the mounts are read from both accounts and merged on the destination. A facet
reading the mount list alone would have lost every tmpfs on the box and said nothing
about it, which is the silent-omission failure this project exists to avoid: a tmpfs
appearing at `/run` or over `/tmp` is exactly the kind of change an operator takes a
fingerprint to catch.

**Keyed by destination rather than listed in the engine's order**, which the merge
needs and the contract wants anyway: on the same measurement two mounts came back in
the opposite order from the one they were declared in, so the order is docker's own
and nobody promised it. A destination is unique per container, and one arriving from
both accounts is docker contradicting itself, so it is refused rather than resolved.

The tmpfs option string is kept whole rather than split into pairs, for the same
reason `/proc/mounts` options are: splitting on every comma corrupts any value that
holds one. Whether the mount is read-only is read from that string, because for a
tmpfs docker keeps it there rather than in a flag of its own.

## A port's bindings are read from what the engine did, not what was asked of it

Measured on docker 26.1.5, publishing one port two ways:

| asked | `HostConfig.PortBindings` | `NetworkSettings.Ports` |
| --- | --- | --- |
| `-p 127.0.0.1:8080:80/tcp` | `HostIp: "127.0.0.1"` | `127.0.0.1:8080` |
| `-p 9000:9000/udp` | `HostIp: ""` | `0.0.0.0:9000` **and** `[::]:9000` |
| `--expose 7777` | absent | `"7777/tcp": null` |

The request understates the reach of the second port in the way that matters most: an
empty host address is not a wildcard until the engine decides it is one, and whether a
port is reachable from the network or only from the box is the line an operator reads
first. So the effective table is what the document carries, keyed by the engine's own
`80/tcp` spelling.

**A port with no bindings is kept, and that is the type's whole reason.** `"7777/tcp":
null` says the container listens on a port nobody published: real state, and a
different fact both from the port being unpublished-and-absent and from the container
not listening at all. Dropping it would lose the difference between an internal
service and no service.

The bindings of one port are sorted, since publishing without an address gives one per
family and the engine promises no order. The host address is the shared `InetHost`, the
same leaf `sockets` reports a listener bound to, so the two facets can be read
together: a port published on `0.0.0.0` with no listener to match is a different box
from one where they agree.

## A container's requested address is kept beside the one it was given

Every network entry carries both, and the pair is the point. A compose file naming a
fixed address is a declaration; what the engine's IPAM did about it is an observation.
They agree almost always, and the almost is the whole reason a fingerprint exists. The
same shape as the postgresql facet's configured port beside the port its running
postmaster reports.

Measured on docker 26.1.5: on a network the container asked nothing of, `IPAMConfig`
and `Aliases` are both `null`, while `GlobalIPv6Address` is `""` on a network with no
IPv6 at all. So an address nobody asked for is recorded as absent rather than as a
request that happened to be honoured, and an empty string never becomes an address that
is nothing.

Aliases are sorted, because they arrive in the order they were declared and that is the
operator's order rather than anything the engine promises.

**Three of docker's fields are deliberately not recorded**, and the reasons differ:

- `EndpointID` is a per-connection handle with no meaning to an operator, and it moves
  whenever a container is reattached.
- `Gateway` and `IPPrefixLen` are properties of the *network*, not of this container's
  end of it, so they belong to the network list rather than to every container on it.
- `DNSNames` is the container's name, its aliases and its own short id, all three
  already in the document under names that say what they are.

The network's id *is* recorded, even though the name is the key: a network destroyed
and recreated under the same name is a different network with a different subnet, and
the id is the only witness to that.

## Unlimited is absent, in all three of docker's spellings for it

Measured on docker 26.1.5, a container given no limits at all reports `Memory: 0`,
`NanoCpus: 0`, `CpuShares: 0`, `PidsLimit: null` and `CpusetCpus: ""`. Three spellings
of one fact, and all three reach the document as absent.

Recording the zero would be a false statement rather than a clumsy one: a memory limit
of `0` reads as a container confined to no memory at all, which is the opposite of
unconfined. The same for the restart policy's `MaximumRetryCount: 0`, which docker
writes both for the policies that have no retry count and for an `on-failure` with
none given, where it means "as often as it takes". Recorded as `0` it would read as
"never retry".

**The limits are kept in the engine's own units, and that is what makes them
recordable at all.** The document admits no floating point, so `--cpus 1.5` could not
be written as a number of CPUs. docker's unit for a fractional CPU is a whole number of
billionths, `1500000000`, so the fraction is carried exactly instead of being
approximated or dropped. Memory is bytes, through the shared `ByteSize`, which refuses
a figure too large to record faithfully at the point it is read rather than letting it
wrap three layers later.

A negative figure is read as no limit too: docker uses `-1` for unlimited swap, and a
negative byte count is not a size.

## The confinement is one node, and it is the effective one

The privileged flag, the capability delta, the confinement options and the shared
namespaces sit together under `security` rather than scattered through the container.
That is the group somebody reads together: an auditor asking what a container can do to
the host wants all four at once, and a diff of that one node answers "did this get
worse".

**The effective options, not the requested ones, and the difference is measurable.** A
container given `--security-opt no-new-privileges` together with `--pid host` comes back
from docker 26.1.5 carrying `label=disable` as well, which docker added itself because
sharing the host's pid namespace makes SELinux labelling impossible. The option nobody
asked for is the interesting one, and only the effective list has it.

**Capabilities are recorded as the delta, not as the resolved set.** The effective set is
the engine's default plus the additions minus the drops, and that default belongs to the
engine's version rather than to the container. Resolving it would mix a decision somebody
made with a default that moves under them, so a docker upgrade would read as every
container on the box having changed. Both lists are sorted, since the engine keeps them
in flag order and swapping two `--cap-add` flags changes nothing about the box.

**The namespace modes are five one-word fields that decide most of what a container can
reach.** `--pid host` lets it see and signal every process on the machine, `--userns host`
makes root inside it root outside it, `--net host` puts it on the box's own stack where
every port it binds is a port on the host. None of that shows in a process table.

Absent where docker writes an empty string, which is a container that chose nothing.
`NetworkMode` keeps whichever of two kinds of thing docker put in it — a namespace choice
like `host` or `none`, or the name of a network — because it is one field in the engine and
splitting it would mean rastro guessing which kind a value is, while a network is allowed
to be called `host`.

`privileged` is recorded even when false. It is the field an auditor reads first, and an
absent false would be indistinguishable from a facet that does not report it at all.

## The healthcheck is configuration, its verdict is an observation, and its log is neither

Three things with the same name, and the facet separates them.

**The check as configured is stable state.** Its command and its four timings do not
move, and a changed interval is a change somebody made. The timings are nanoseconds,
docker's own unit, which is what lets `--health-interval 30s` be recorded exactly in a
document that admits no floating point.

**Its verdict is volatile.** `healthy` becoming `unhealthy`, and the failing streak
counting up, happen on their own on a box nobody touched, so both are annotated and sit
in `state` beside the container's status. That pairing is the useful one: a container
that is `running` and `unhealthy` is the case an operator is looking for, and one word
without the other does not say it.

**Its log is dropped, and it is the only field here that is dropped rather than
annotated.** docker keeps the last few runs of the check together with their output, and
the output of a failing database check is its connection error, credentials and all. It
is also a rolling window that changes on every run. There is no reading of it that
belongs in a fingerprint, so the deserializer does not declare the field at all: not
asking is how it stays out.

The log *driver* is recorded, with its options, because an unbounded `json-file` is how a
box fills its disk and the difference between that and the same driver with `max-size`
set is invisible unless both are there. A container docker reports no driver for is on
`json-file`, which is the engine's own default rather than a guess.

## Images are keyed by id, which is the opposite of how containers are keyed

Containers are keyed by name because a name outlives the id it is minted with. Images
are keyed by id for the mirror-image reason: **a tag is not identity, and moving one is
the event worth catching.** `nginx:1.29` repointed at a rebuilt image leaves the old
image on the box with no tags and gives the new one the tag, and only an id-keyed table
shows both halves of that at once. Keyed by tag, the same event would read as one entry
whose contents changed, which says less.

**A dangling image is kept.** `docker image ls --all` includes the images a rebuild
displaced, and they are state: they hold disk, they are usually an accident, and
`<none>:<none>` is the only place an operator ever meets them. An entry with an empty
tag list says exactly that.

**The labels are read for the provenance.** `org.opencontainers.image.revision` names
the commit an image was built from, which on a box running images nobody can rebuild
from memory is the only link back to the source. Measured on two builds of the same
Dockerfile: the tag moved, the revision label changed, and the displaced image kept the
old one.

**What is deliberately left out of an image**, each for its own reason:

- the image's own `Config`: those are container defaults, and every container running
  the image already reports them resolved, environment included;
- `RootFS.Layers`: content the id already addresses, and a list per image on a box with
  eighty of them;
- `Metadata.LastTagTime`: it moves when somebody re-tags rather than when anything about
  the image changes.

The size is required rather than optional, unlike a limit: an image always has one, so a
negative or unreadable figure is a misread and fails, where an absent limit is a fact.

## A volume's driver options are read, because the mountpoint can be a lie

A `local` volume created with `--opt type=tmpfs --opt device=tmpfs --opt o=size=32m`
still reports a mountpoint under `/var/lib/docker/volumes/`, and the data is not durably
there at all. An NFS volume is the same shape: the mountpoint is local and the options
name the server the data actually lives on. Measured on docker 26.1.5.

A facet that recorded the mountpoint alone would describe the wrong place with
confidence, which is worse than describing nothing. So the driver, the options, the
labels and the scope are all read, and the mountpoint is one value among them rather
than the answer.

**Volumes are read in their own right rather than only as a container's mounts.** They
outlive the containers that used them: a volume left behind by a container that has been
deleted is invisible from every other part of this facet, and it is simultaneously where
a box's data is and where its wasted disk is. An anonymous volume, which a container gets
when an image declares `VOLUME` and nobody named one, is recorded like any other under
its 64-character hex name, for the same reason a dangling image is.

## A network's own record is read, and the containers on it are not read twice

`docker network inspect` lists every container attached to the network, and every one of
those containers already reports the network from its own end, with more: its aliases and
the address it asked for. Recording the same edge from both ends would give a reader two
places to disagree about one fact, so the network's entry holds no container list. The
same reasoning that keeps a container's `DNSNames` out: it is the name and the aliases
already recorded, spelled again.

What the network's own entry carries instead is what only it knows:

- **the addressing**, because the subnet is what every container's address on it has to
  fall inside, and a network recreated with a different subnet moves every container at
  once;
- **the driver options**, because for a bridge they decide what is permitted:
  `enable_icc` whether containers on it can reach each other at all,
  `host_binding_ipv4` which host address an unqualified `-p` publishes to, and `name` the
  host interface the network actually is;
- **`internal`, `attachable` and `ingress`**, one word each, deciding whether there is a
  route off the box, whether a standalone container may join, and whether this is the
  network a swarm publishes services through.

The engine's own three — `bridge`, `host` and `none` — are included rather than filtered
as built-ins. The default bridge's `enable_icc` governs every container that chose no
network, and nothing else in the document says so.

**One measurement corrects a claim made earlier in this work.** A network created with a
subnet and nothing else reported *no* gateway in the IPAM config immediately after
creation, and reported `172.30.0.1` in it after the daemon restarted. Same docker, same
network. So the gateway is optional because the engine is inconsistent about echoing it,
and an absent one means unreported rather than none. The code said the first thing as if
it were the whole rule, and now says both.

## The engine's private trees are sealed by listing them, not by naming them

On a box running containers this is where the filesystem walk spends itself. Measured
twice:

- a development machine: **376,948** of its **834,466** entries were under the container
  store, and **294,525** of those were layer entries;
- the reference container, after the claim: **6,942** entries on disk under the engine's
  root against **20** recorded, the layer store reduced from 6,761 entries to the one
  entry for its own directory, and all 8 entries of the volume tree intact.

**The trees are resolved by listing the root's children rather than by naming them, and
that is not tidiness.** The layer store's directory is `overlay2` under one driver and
`vfs` under another, and on docker 29 — whose driver reports itself as `overlayfs` —
there is no `overlay2` directory at all: the layers are under `rootfs` and inside
containerd's own store. So a fixed list of names would have been wrong on docker 29, and
a mapping from the driver name would have been wrong in a different way. Listing what
the engine actually keeps covers every driver, every version, and a directory a later
docker adds without rastro being told.

**Sealed rather than merely unhashed**, on the reasoning the postgresql data directory
established: it is most of the entries, every attribute that survives moves on the next
pull, and what is genuinely in there this facet reports properly — the images by digest,
the containers by name, the volumes by name and driver.

**One directory is named, and it is the one that must survive**: `volumes`. That is where
a box's databases, uploads and certificates live, and it is the only tree under the
engine's root that is not the engine's own bookkeeping. It carries no claim at all, so it
is read exactly as the walk reads anything else.

**Sealing the root and sparing volumes underneath it is not available**, and it was
checked rather than assumed: the walk prunes at a sealed directory, so a rule for a
subtree of one is never consulted. That is why the claim is one per child rather than one
for the root, and it is also why the choice matters — a config can only narrow, so a
sealed root would have removed the operator's data from the document with no way to ask
for it back.

**The root is resolved when the collector is constructed**, from `docker info`, because
the walk's table is built before any collector runs and a claim cannot wait for the
facet's own read. That is the arrangement the postgresql collector already uses for its
cluster list. An engine whose daemon did not answer names no root, and then no claim is
made at all, because the walk's own reading is the safe direction to be wrong in.

## containerd is asked where it is listening, because `ctr`'s default is wrong on a docker box

Measured on docker 29.8.0 with containerd 2.3.4 underneath it: containerd runs as
`containerd --config /var/run/docker/containerd/containerd.toml`, its socket is
`/var/run/docker/containerd/containerd.sock`, and `/run/containerd/containerd.sock` — where
`ctr` looks when nobody tells it otherwise — **does not exist**. A bare `ctr` there fails
outright with `cannot access socket`.

So the running process is asked, in the order it can answer:

1. its own `--address`, in either spelling, which is the whole answer when it is there;
2. the `[grpc] address` of the file its `--config` names;
3. containerd's documented default, for a containerd started with neither.

The process is identified by the binary behind it rather than by a name, the same way the
`exporters` facet identifies an agent: a unit may be called anything, and the executable
is the fact.

**The configuration is parsed by naming the two lines that matter, and that is not
fastidiousness.** The first `address =` in the file docker's containerd is given belongs
to `[debug]`, and the debug endpoint answers a different API. Anything taking the first
match would talk to the wrong socket and report the failure as though containerd were
broken.

**This is a discovery read, not a state read**, which is why parsing a configuration here
does not need the licence the nginx entry grants. It establishes how to reach the service;
what the service then says about itself is asked of the service.

A configuration rastro cannot read — which an unprivileged run makes ordinary — falls back
to the documented default rather than giving up: the engine is plainly running, and `ctr`
says so loudly if the address is wrong. A box with no containerd process gets no address at
all, because the default is not worth guessing when nothing is behind it.

## containerd is a second dialect, not the same engine spelled differently

The facet's three-part shape is shared with docker — the client that is installed,
whether anything answered, and what it said — because those are the three states a
reader has to tell apart whichever engine it is. Almost nothing inside is shared, and
that is the point of keying the facet by flavour rather than flattening both into one
container type.

**`ctr --version` is the read for the client, and `ctr version` for the server.**
Measured on containerd 2.3.4: the `version` subcommand has to reach the socket to answer
and, against an address with nothing behind it, exits non-zero printing *nothing at all* —
not even the client's own half. `ctr --version` never connects and answers regardless. So
the client's version is readable on exactly the box whose state is hardest to describe:
containerd installed and stopped.

**A successful `ctr version` with no server block is a failure, not an absence.** Since a
`ctr` that cannot reach containerd exits non-zero, and the execution seam turns that into
a recorded failure, output that *did* succeed and carries no server block means the format
is not the one rastro reads. That is precisely the day this has to be loud rather than
report an engine with no version.

**Everything else avoids `ctr`'s tables.** `ctr` calls itself a debug tool and promises
nothing about its output, so every other read uses `--quiet`, which prints one identifier
per line, or the JSON of `containers info`. The version is the one place with neither.

**The revision is recorded beside the version, and it earns its place here more than it
would for docker**: containerd's version moves slowly and a distribution's rebuild changes
only the revision, so the version alone would call two different builds the same engine.
Debian 13's containerd reports `1.7.24~ds1` with revision `1.7.24~ds1-6+deb13u1`, which is
a package version rather than a commit, and is recorded as reported.

**Detection is the client, as it is for docker.** `ctr` ships with containerd, so a box
that has it has had containerd installed, and whether anything answers is then state.
**Cost, accepted knowingly:** a containerd running with no `ctr` installed is not reported
at all. That is a limit of rastro rather than a fact about the box, and the same one docker
has if its client is missing.

**containerd's own store is not claimed yet.** On a docker box its layers are inside the
tree docker's root already seals, and a standalone containerd wants its own measurement
before a claim is made against it.

## The two engines' container lists differ in population, and that is the evidence for keeping them apart

Measured on one box, docker 26.1.5 with its managed containerd 1.7.24, at one moment:

| view | containers |
| --- | --- |
| the `docker` entry | 7, running and stopped alike |
| the `containerd` entry, namespace `moby` | 2, both running |

docker deletes the containerd record when a container stops and keeps its own metadata,
so containerd's list holds only what is running. Neither view is wrong and neither is a
subset worth suppressing: "docker has forgotten a container containerd still holds" and
"docker holds a container containerd has never heard of" are both real states, and only
two entries side by side can show either.

**The namespace is the outer key**, not a field on each container, because it is
containerd's tenancy boundary: two namespaces may hold the same id and nothing in one is
visible from the other. Which namespaces exist is itself a fact about who is using the
engine, so an empty namespace keeps its key.

**Containers are keyed by id here and by name in the docker entry**, and the asymmetry is
containerd's: it has no names. A container's id is whatever created it chose — docker and
a kubelet use a hex string, `nerdctl` uses the name the operator typed — so there is no
second identifier to prefer.

**A container's image is optional, and both shapes were measured.** On docker 26.1.5 the
containerd record's `Image` is empty, because docker keeps its own snapshots and hands
containerd a prepared rootfs; on docker 29.8.0, whose snapshotter *is* containerd's, it
holds `docker.io/library/alpine:latest`. The same goes for the snapshotter and its key,
empty on the first and set on anything created through containerd itself.

**`tasks ls` is the one `ctr` table this collector parses, and it is read once per
namespace.** containerd offers no `tasks info`, so the pid and the status exist nowhere
else, and the table answers for every container in the namespace at once. Its three
columns are an id, a number and a single word, none of which can hold a space; a row with
any other number of columns is refused rather than guessed at.

**A container with no task is defined and not running**, which is the state docker spells
as a status on the container itself. Here the absence of the task *is* the status, which is
why the task is optional rather than a status word that is sometimes empty.

## `ctr`'s one table is sliced by its header, because a column holds two words

`ctr images ls` is the only read in this collector with neither a `--quiet` form nor JSON,
and its columns cannot be split on whitespace: the size prints as `3.9 MiB`, two tokens in
one column, so a positional split puts the platforms where the labels belong and shifts
every field after the size.

The header is padded to the width of the widest cell in each column, which makes its own
column offsets the authority on where each field starts. So the table is *sliced* by the
header rather than split, read by column name, and a column containerd adds later shifts
nothing.

**Two of its columns are read and two are not, each for its own reason.**

- The size is not recorded: `3.9 MiB` is a rounding, and there is no `images info` to ask
  for bytes. A rounding in a diffable document changes when the formatting does and not
  when the image does.
- The labels are not recorded: the only form is one comma-joined cell, and a label's value
  may itself hold a comma, so splitting would corrupt values rather than read them.

docker's own image entry carries real bytes and structured labels, which is the point of
keeping the two dialects apart rather than pretending to one shape.

**And whether containerd holds any images at all depends on what is driving it**, measured
on both: docker 26.1.5 keeps its own image store and uses its containerd only as a runtime,
so its `moby` namespace holds containers and **no** images; docker 29.8.0, whose snapshotter
*is* containerd's, holds both. An empty image map beside a populated container map is
therefore a real reading of a real box rather than a failed one.

## containerd's own trees are sealed as two claims, and the paths are resolved first

containerd keeps what it holds in two places, both named in its configuration and both
read in the same pass that finds its socket: `root`, the content store and the snapshots,
and `state`, the shims, sockets and task directories of what is running. A containerd that
names neither is at the documented defaults, `/var/lib/containerd` and `/run/containerd`.

**Two claims rather than a listing, which is where this differs from docker.** docker keeps
the operator's volumes inside its own root, so its children have to be claimed one at a
time to spare that one. Nothing under containerd's root or its state belongs to the
operator, so the trees themselves are sealed and the walk stops at each. The docker
arrangement is the pattern for an engine that mixes its own store with the operator's
data, not a rule that generalises: podman keeps `graphroot`, `runroot` and `volume_path`
as three independently relocatable roots, and will need its own reading of the same
question.

**Every claimed path is resolved through its symlinks first, and that was a trap worth
measuring.** docker gives its managed containerd
`state = "/var/run/docker/containerd/daemon"`, and on Debian `/var/run` is a symlink to
`/run`. The filesystem walk never follows a symlink, so it only ever records the real
path: a claim naming the symlinked one is a rule about a tree nothing visits, and it would
have failed silently. On the reference box the claim is now recorded as
`/run/docker/containerd/daemon`, which is where the walk goes.

**One tree is claimed once, whichever dialect resolved it.** Two claims on one path fail
the *walk* rather than one facet, and two engines legitimately resolve to the same
directory: docker's managed containerd keeps its store at
`/var/lib/docker/containerd/daemon`, inside docker's own root. Two dialects saying the same
thing about one tree is not a disagreement, so the collector folds it rather than reporting
it.

## podman does not come through the gate, and its CLI never will

The nginx entry says a service's own account of itself may be read only where asking does
not change the host, **with the measurement attached**. It named podman as one that had to
be measured before its dialect was written. Here is that measurement, and podman fails it
twice over.

**A read initialises a store.** `podman ps --all` against an empty store, under a cleared
environment, created **22 filesystem entries**: `db.sql`, five lock files
(`storage.lock`, `userns.lock`, `layers.lock`, `containers.lock`, `images.lock`), the
`overlay`, `overlay-layers`, `overlay-containers`, `overlay-images`, `volumes` and `libpod`
directories, and `overlay/.has-mount-program`. On a box where podman is installed and has
never been used, a fingerprint run would create a container store and then report the box
it had just changed.

**And on an initialised store it writes when the store needs work**, which is worse than
writing always. Three measurements, each against a zero-change control:

| local `podman`, store state | changed |
| --- | --- |
| empty | **22 created**: `db.sql`, five lock files, the driver's directories |
| overlay driver, initialised | **8**: `overlay/volatile-true` and `overlay/idmapped-lower-dir-true`, probes podman writes to test what the filesystem supports, plus `storage.lock`'s stamps |
| vfs driver, initialised | **0** across five reads, and it succeeds with the store made read-only |

**The last row is not a reprieve.** A mutation that depends on the driver and on what is
already there is harder to defend than a constant one, because the box it changes most is
the box nobody has ever run podman on — and that is exactly the run every later comparison
is made against. A first fingerprint that creates a container store is a first fingerprint
of a box that no longer exists.

**The mechanism, which is the part worth keeping.** podman is daemonless, so in local mode
there is no engine until the command starts one: `podman ps` opens the store, takes the
locks, initialises the graph driver, opens or creates the database, probes the filesystem,
answers, and exits. Every one of those writes is part of *being* the engine rather than
part of reading it. That is also why `nginx -t` is the nearer comparison than it first
looks: both are a tool doing the work of the service in order to answer a question about
it.

**Corrects an earlier reading in this same entry.** It first said every read on an
initialised store writes. That was measured on an overlay store and generalised, and the
vfs measurement above disproves the general form. The entry is corrected here rather than
in a new one because nothing has been released against it, and because the corrected fact
strengthens the decision rather than reversing it.

**The consequence, and it is a design position rather than a delay.** There is no podman
dialect in this facet, and there will not be one built on `podman`. A box running podman
reads as `absent`, which is a limit of rastro rather than a fact about the box, and this
entry is what an operator gets pointed at.

**Three routes are open, and one of them is the CLI in a mode that cannot write.**

- **`podman --remote`, which is the answer and needs no HTTP client — but only where a
  service is already running.** The same binary in
  remote mode is a pure API client, the thing `podman-remote` is, and it never links the
  store path at all: `--root` is rejected there as an unknown flag, because the flag belongs
  to code that is not in play. Measured on a quiet box with a service running, five reads —
  `ps`, `images`, `volume ls`, `network ls`, `info` — changed **nothing**, against a
  zero-change control. Measured again with no service *and* no store, it exits 125, creates
  no store and changes nothing.

  **Detection is the service's process, not its socket, and that distinction is the whole
  care.** `podman.socket` is socket-activated: on the reference machine the socket unit is
  *enabled* and the service unit is *disabled*, `TriggeredBy=podman.socket`. So the socket
  file exists whether or not anything is listening behind it, and connecting to it makes
  systemd start `podman system service` — which then opens the store, creating it if it is
  not there. Connecting on spec would cause exactly the mutation the local CLI was refused
  for, one step removed and harder to see. A `/proc` scan for a running service is a pure
  read and answers the real question, so that is what detection does.

  **And "podman is running" is usually false on a box full of running containers.** podman
  starts a container and exits; `conmon` and the OCI runtime keep it alive. The reference
  machine had 17 containers, 18 `conmon` processes and *no* podman process doing that work —
  the one it had was a service its macOS client talks to, which is an artefact of
  `podman machine` rather than anything a podman host normally has. So the common case is a
  box where podman is installed, containers are up, and there is nothing to ask.
- **Its store, read as files.** The design already grants this shape for apk: read a
  manager's own database where the tool offers no format rastro controls. podman's is
  SQLite with no schema contract, so it is the weaker of the two.

Reversing this means a new entry with a measurement showing a read that leaves the box as it
was found.

## The rest of a container's definition, and the three ways docker spells "none"

Devices, ulimits, kernel parameters, name resolution and the shared-memory size complete
what a container was defined with. Each earns its place by being invisible everywhere else:

- **a device** is the sharpest thing a container can be handed short of privilege.
  `--device /dev/sda:/dev/sda:rwm` gives it the disk the host boots from, and the mount
  table does not show it. Keyed by the path inside the container, which is unique there;
- **a ulimit** is recorded as both halves, because the soft limit is what a process starts
  with and may raise, and the hard one it cannot;
- **the kernel parameters are the container's own**, which is a different fact from the
  `sysctl` facet's reading of the running kernel: one is a request in a definition, the
  other is what the box is currently set to. That is also why they are not the `sysctl`
  facet's value objects, whose volatility and secrecy rules are about a runtime reading;
- **name resolution** decides what a container can reach, and an `--add-host` entry is a
  name that resolves nowhere else on the box. The lists keep the engine's order, because a
  resolver list is ordered and sorting it would change what the container does. An added
  host is split on its *first* colon only: an IPv6 address is full of them, so
  `db:2001:db8::1` is one name and one address rather than four fields;
- **the shared-memory size** is recorded even at docker's default of 64 MiB, because a
  container given `--shm-size 1g` differs from one that was not.

**One `HostConfig` spells "none" three different ways, measured on 26.1.5**: an empty array
for `Devices`, `Ulimits` and the three DNS lists, `null` for `Sysctls` and `ExtraHosts`,
and `0` for a limit. All of them read as nothing here, and the tests hold a container with
none of them beside one with all of them so a future change cannot quietly conflate the
spellings.

**Device requests are still owed, and deliberately not guessed.** They are how a GPU
reaches a container, and there is no GPU on any box this collector was built against. A
shape written from the API reference rather than from a run is precisely the mistake the
fixtures here exist to avoid, so the field waits for a box that has one.

## One test asks docker, and it found something on its first run

Every other test of this facet asserts what rastro does with output captured once, which
pins the code against the author's own reading of that output and cannot catch a fixture
captured wrong. `tests/containers_conformance.rs` asks docker instead: the container names
against `docker ps --all`, the image ids against `docker image ls --all --no-trunc
--quiet`, the volume names against `docker volume ls`, and the sealed trees against the
directories under the root `docker info` names.

It needs a live engine with something on it, and **fails rather than skipping** when there
is none — a check that quietly passes on a box with no docker is how a whole dialect could
rot unnoticed. `.github/workflows/live-engine.yml` provides both, in the
`extended-verification` tier. The container suite cannot: it runs *inside* a container, and
a dockerd in there needs privilege the other legs deliberately do not have.

**It earned its place immediately**, the way the nginx conformance check did. Three of its
four comparisons passed and the fourth caught a rule that could never apply: on a docker
box the managed containerd keeps its store at `/var/lib/docker/containerd/daemon`, inside
docker's own sealed root, so that claim sat in the effective table matching nothing the
walk could ever visit. The collector now folds a claim contained by another, and the
containerd facet reports its root and state as values so nothing is lost by the folding.

**The oracle for the claim is the root, not `GraphDriver`.** The obvious check was to
assert the sealed tree holds the layer path `docker image inspect` reports in
`GraphDriver.Data` — but measured, that field is `{"Data":null,"Name":"vfs"}` on docker
26.1.5 and `null` outright on 29.8.0, so on both engines available there is nothing to
compare. Listing the root's children needs neither an image nor a driver-specific field,
and it checks what the claim actually promises: every directory the engine keeps to itself
is sealed, and the one holding the operator's volumes never is.

## Talk to an engine, never be one

The podman measurements above are an instance of a rule worth stating on its own, because
it decides how every future engine is read.

**A client cannot change the host; an engine must.** docker's CLI has never written
anything during a read, and neither has `ctr`, because both are clients: they open a
socket, send a request, and decode the answer. podman's CLI in local mode writes because
it is not a client of anything — it is the engine, assembled for the duration of one
command and torn down after. The store it opens, the locks it takes and the capability
probes it leaves are what being an engine consists of.

So the question to ask of a new engine is not "is this tool read-only?" but **"is this
tool a client, and is there something for it to be a client of?"** Where the answer is no,
there are only two honest moves: read the engine's own files, which cannot mutate, or
report the engine as present and unread and say why.

This also explains the nginx entry better than that entry did. `nginx -T` creates log
files not because testing a configuration is careless, but because a configuration test is
nginx *doing the work of starting* in order to answer a question about starting. Same
shape, different subsystem.

**What it means for the facet.** rastro may run a tool that asks an engine, and must not
run a tool that becomes one. `docker`, `ctr` and `podman --remote` are on the first side.
Local `podman` is on the second, and no flag moves it across.

## podman is read through a service somebody else is running, and claimed from its files

The refusal above stands: podman's local CLI never comes through the gate. What comes
through instead is `podman --remote`, which is the same binary acting as a client, plus a
filesystem claim that needs no podman at all. Three parts, each measured.

**One local call, and it is a flag rather than a subcommand.** `podman --version` on a
wiped box created nothing; `podman version` created 22 entries. The difference is not
cosmetic: the subcommand reports a *server* version, and in local mode producing one means
becoming the server. So the client's version is read with the flag and everything else goes
to the service.

**The service is found by its process, never by its socket.** `podman.socket` is
socket-activated — on the reference machine the socket unit is enabled and the service unit
is *disabled*, `TriggeredBy=podman.socket` — so the socket file exists whether or not
anything is behind it, and connecting to it makes systemd start the service, which then
opens the store. A `/proc` scan for a running `podman system service` is a pure read and
answers the real question. Its address comes from its own command line where it names one,
and from the documented default where systemd handed it the socket instead.

**"Installed and unread" is the ordinary state, not a failure.** podman is daemonless, so a
box can be full of running containers with no podman process at all: the reference machine
had 17 containers, 18 `conmon` processes and no podman doing that work. The entry records
the client version, `service: unreachable`, and the reason in full, so a reader is never
left wondering whether rastro looked. Verified on a live box: with the service stopped, the
read changed **0** entries in podman's store.

**The claim is built from podman's configuration rather than from podman**, which is what
lets it exist in that state — and it is the part that matters most for the walk, since the
store held 376,948 of one machine's 834,466 entries. `graphroot` and `runroot` come from
`storage.conf`, distributed defaults first and the operator's second, falling back to the
documented paths when neither names them, which is the ordinary case: neither file sets a
root on a stock box.

**The volume tree is discovered rather than named, which is where this differs from
docker.** docker always keeps volumes inside its root, so that one name is hardcoded.
podman's `volume_path` lives in `containers.conf` and may sit outside the store entirely;
where it does, every child of the store is sealed and the volumes are left where the walk
finds them.

**Still owed:** rootless podman. Each user's service listens at
`/run/user/<uid>/podman/podman.sock` with its own store, and root can connect to all of
them. That means one flavour with several instances, which the facet's one-entry-per-flavour
shape does not hold, so it needs a shape decision before it needs code.

## podman's containers come from one list, and its shapes are its own

`podman --remote ps --all --format json` carries what docker needs an `inspect` per
container to say: the image, the state, the ports, the labels, the pod. So there is no id
list to race against here and no per-container loss to record, which is why podman's entry
has no `unreadable_containers` beside docker's.

Three shapes differ from docker's, and each is recorded as podman spells it:

- **Timestamps are whole seconds since the epoch**, not RFC 3339 text. The key names carry
  the unit the way the filesystem facet's do. A running container's `ExitedAt` is
  `-62135596800`, Go's zero time in seconds, which is the same trap docker sets with
  `0001-01-01T00:00:00Z` in a spelling that would read as the year one.
- **An image id is bare hex**, where docker writes `sha256:` and the hex. `ImageDigest` now
  accepts either and normalises neither, because rewriting podman's into docker's would mean
  asserting an algorithm the engine never named.
- **A published port keeps its range.** `-p 8000-8010:8000-8010` is one binding covering
  eleven ports in podman and eleven bindings in docker. Flattening podman's would mean
  inventing ten entries it never reported.

**Pods are recorded because podman has them and docker does not.** A pod is a group of
containers sharing namespaces, and its infra container is the one holding them open, so a
container that belongs to one is reachable in ways a standalone container is not.

**One name per container, and more than one is a misread.** podman's `Names` is a list for
docker compatibility and holds exactly one, so a second is not a container with two names:
it is the answer not being the shape rastro thinks, and it fails rather than picking one.

## Containers are hoisted out of the engine that runs them

The facet has two halves, `engines` and `containers`, rather than one tree of engines with
their containers inside them.

**The rule behind the split: a container is a tenant of the box, an image is an artefact of
the engine's store.** What an operator asks a fingerprint first is what is running here,
and answering it should not require knowing which engines the box has. What stays with the
engine is everything that is not a container — its images, its volumes, its networks, its
version, and the account of whether it could be read at all.

**The engine is still the first key under `containers`, and that is not a compromise.**
`docker/web` and `podman/web` are two different containers that share a name; containerd has
no names at all and keys by id, one namespace deeper. A flat list keyed by container name
would collide on the first and be impossible for the second, and a flat list with a union of
every dialect's fields is the thing this facet refuses everywhere else.

**The losses stay with the engine.** A container that vanished between the id list and the
read of it is a fact about the *read*, not about the containers that survived, so
`unreadable_containers` sits beside the daemon status that describes the same read.

**One model, two views, not two copies.** The engine still owns its containers in the
model — a server holds what it is running — and the document renders that ownership twice:
once as the engine's entry without them, once as the containers half. Nothing is duplicated
in the document, so there is no second place for a diff to disagree with itself.

## A flavour holds instances, keyed by the account that owns the engine

`containers/docker/root/web` rather than `containers/docker/web`, on both halves of the
facet.

**One flavour is not one engine, and podman is why.** Every user can run their own
`podman system service` with its own store, its own socket and its own containers, so
`alice`'s `web` and `bob`'s `web` are different containers and root may be running none at
all. A key that held "the podman on this box" would have to pick one of them and call it
the engine, which is a statement about the box that is not true.

**The owning account is the identity because it is what separates them**: the store, the
socket and the containers all belong to that user, and the name reads well in the ordinary
case, where a box has exactly one instance and it is `root`.

**It generalises past podman**, which is the other reason to spend a key on it. Rootless
docker has the same shape, and so would a box where root runs dockerd while a user runs
their own. Adding the level later would have been a second change to the output contract,
which is the sort of thing to do once.

**Cost, accepted knowingly:** one more level for every box that will only ever have one
instance, which is most of them. The alternative was a key whose meaning changes depending
on what the box happens to run, and that is the worse of the two.

## A rootless engine is found by whose process it is

podman's per-user services are discovered the same way root's is — by looking for a running
`podman system service` — with one addition: the account that owns the process, taken from
the uid of its own `/proc` directory. That is one `stat` rather than a parse of `status`,
and the kernel sets it to the process's real uid.

**Each instance is read as that user's engine, not as root's with a different socket.** A
rootless podman is configured somewhere else and defaults somewhere else: its store is under
the user's home rather than in `/var/lib`, its runtime state is in their runtime directory,
and their `~/.config/containers` overrides the system files rather than being overridden by
them. A layout that used root's paths would name root's store as theirs.

**The account's name comes from `/etc/passwd`, read here rather than taken from the
`accounts` facet**, because a collector may not read another collector: what one facet
knows is not a channel the others may use. It is three columns of a file every Unix has, and
it exists so the document says `alice` rather than `1000`. A uid nobody named falls back to
the number, which is still true where saying nothing would not be.

**Root's engine is reported whenever the binary is there; a user's only when their service
is.** Root's store and its claim exist whether or not anything is running, and rastro can
read the configuration for it. For a user with no service there is nothing rastro can learn
without changing their box, and inventing an entry from the defaults would be describing a
store that may not exist.

Verified on a live box: `alice` and `root` discovered as separate instances, answering at
`/run/user/1001/podman/podman.sock` and `/run/podman/podman.sock`, with stores under
`/home/alice/.local/share/containers/storage` and `/var/lib/containers/storage`, and their
containers kept apart.

## A logging option's value is withheld, like an environment value

The log driver and its option *names* are recorded plainly; every option *value* is marked
sensitive, so the default view carries a digest rather than the text.

**Because some drivers require a credential there.** docker's splunk driver takes
`splunk-token`, and the gelf and fluentd drivers take an address that can carry one. The
names are open-ended — a logging plugin defines its own options — so a rule that judged by
key would have to enumerate every driver's and would be wrong about the next one. That is the
reasoning the container environment already gets, and the same answer: the keys stay public,
the values do not.

**Cost, accepted knowingly:** `max-size` and `max-file`, which are the options an operator
actually reads, arrive as digests too. A diff still says they changed, which is most of what
the fingerprint is for, and `--raw` is where the rest is paid back once it exists.

## docker has two spellings of an empty `HostConfig` collection

Measured, because it decides how every one of these fields is declared. On docker 26.1.5 a
container with nothing there reports `"CapAdd": null`, `"SecurityOpt": null`, `"Binds": null`
and `"ExtraHosts": null`, while `Tmpfs` and `Sysctls` are **absent from the document
entirely**. The 29.8.0 fixtures show the other half: there `Sysctls` and `ExtraHosts` arrive
as explicit `null`.

**So both spellings have to be tolerated where either was seen, and `#[serde(default)]`
alone does not.** It covers a field that is missing; an explicit `null` into a bare
`BTreeMap` or `Vec` fails the *whole* document rather than the one field, so the container is
recorded as unreadable and nothing else about it survives. Every field measured to arrive as
`null` is `Option<...>` with `default`, read through `.flatten()`. `Tmpfs` was the one that
was not, and a test now holds it.

**The fields measured to always arrive as a list or a map are left as they are**, which is
the standing rule here: what the host was seen to do, not what it might. The residual is
named rather than guessed away — a docker that one day writes `null` where 26.1.5 and 29.8.0
both write `[]` costs that container's entry, loudly, in `unreadable_containers` with serde's
own message. That is the failure this facet is shaped for, and it is a better trade than
declaring shapes nobody has observed.
# `--raw`, and a document that admits which one it is

Dated 2026-09-08. Finishes the mechanism
[Redacting a sensitive value](#redacting-a-sensitive-value) left half-built, which that
entry closed by naming `--raw` as not built.

`--raw` sets `Disclosure::Raw` for the run. It prints a warning on stderr first, and the
warning names what the file now holds rather than the flag that caused it: the operator
typed the flag, so repeating it back tells them nothing, whereas "this document carries
the box's secrets in cleartext" is the fact that decides whether they pipe it anywhere.

**The disclosure is in the `invocation` facet, beside the view, as `config.disclosure`.**
Same argument the view is there for: each axis rewrites the document wholesale, so
diffing a `--raw` fingerprint against a redacted one would report a changed value at
every sensitive field with nothing in either document to explain it. Two keys rather than
one, because the axes are independent — a complete view says nothing about whether a
secret in it was withheld, which is the whole reason `Disclosure` is not another value of
`View`.

**`Cli::view()` became `Cli::presentation()`.** One accessor, because `Presentation` is
the pair and nothing downstream wants half of it, and because building it through
`From<View>` and then opting out keeps redaction-by-default structural here too: a branch
that forgot `--raw` entirely still produces the safe document.

**The port grew two re-exports rather than the collector reaching around it.**
`invocation` needs `Presentation` and `Disclosure` to report them, and
`tests/purity.rs` forbids a built-in collector naming `rastro_fingerprint` — under a
comment that names the fix as widening the port. `View` was already re-exported for the
same reason, so this is the established shape and not a new hole in it.

**The `invocation` collector's version stays at `1`.** Not a special case for metadata: no
collector moves before the first release, which
[a later entry](#every-collector-is-version-1-until-rastro-has-a-release) makes the general
rule and applies to the six that had already moved.

**Cost, and it is the real one:** a fingerprint taken before this change and one taken
after differ in the `invocation` facet on an unchanged host, and nothing in either document
says why. That is accepted rather than solved, because the alternative prices a
pre-release format change as though the format were already published.

**Withdrawn: "`--raw` is not built" as a reason to defer.** Two entries rest on it and
their decisions still stand, but the premise no longer does. Neither is reversed here and
both are now revisitable on their merits:

- [A facet's error text is not classified, yet](#a-facets-error-text-is-not-classified-yet)
  deferred on the mechanism not existing. It exists. Whether diagnostic text is an
  observed value is now a question that can be answered rather than postponed.
- [The postgresql role digest is not marked sensitive](#redacting-a-sensitive-value)
  named the harder case, and that case is unchanged by this work: the verifier is not in
  the process to render, so opting out of withholding it means the collector asking the
  server a different question. That is a query change, not a render-time decision, and
  `--raw` still does not cover this facet. Recorded here so the gap is not read as an
  oversight now that the flag exists.

# The `units` facet reports what a unit sets in the environment

Dated 2026-09-08. Driven by a question a file-copy migration raises: an application that
needs a variable keeps working only if whatever sets it came across too, and a walk of the
filesystem shows that `~/.bashrc` moved without saying what it set.

**Environment belongs to whatever carries it, not to a facet of its own.** `cron` already
reports the variables a crontab sets, for reasons its own model states. A unit's
`Environment=` is the same concept on the other carrier, so it is reported by `units` and
not by a new collector that would have to know how both work. The alternative considered
and rejected was an `environment` collector with hooks the other collectors call, which
inverts the dependency direction this repo keeps one-way — a collector never knows another
exists, and the one cross-collector mechanism that does exist works the other way round:
collectors *declare* filesystem claims and the composition root gathers them.

`EnvironmentVariableName` moved out of `cron` and into `rastro-collector`, which is what the
port's own rule asks for — a value earns its place there by having consumers in more than
one collector. It is the only shared machinery the idea needed.

**Names in the clear, values `sensitive`.** A name says which variable a service depends on,
which is exactly the migration finding and is not itself a secret. A value is where a
database password lives on most boxes that have one. Redaction still diffs, so a rotated
credential shows as a changed digest without the document carrying it. This is the first
facet where the annotation is doing the job it was built for on a value an operator would
actually want back, which is why `--raw` landed first.

## What `systemctl show -p Environment` actually prints, measured

Against systemd 257 on Debian 13, in a container running real systemd, with throwaway
units written and shown but never started. None of this is in the documentation in a form
that could be relied on, and two of the five would have been got wrong by inference.

- **One line**, however many the unit file spread the setting over. Entries are separated
  by spaces and quoted only where they need to be: `SIMPLE=plain "SPACED=two words"`. The
  quotes wrap the whole `NAME=VALUE`, not the value.
- **Split on the first `=` only.** `EQUALS=a=b=c` is one variable.
- **An empty value is legal**, and a unit that sets nothing prints `Environment=` with
  nothing after it. A unit with no `EnvironmentFile=` prints no `EnvironmentFiles=` line at
  all, so absence arrives differently on the two properties.
- **systemd C-escapes what it shows, and the escaped spelling is not the value.**
  `NEWLINE=a\nb` on the wire is a real line feed in the process, and `BACKSLASH=a\\b` is one
  backslash. Measured by starting a unit with `ExecStart=/usr/bin/env` and reading the
  bytes, rather than by reasoning about the format. Recording the wire spelling would have
  put a value in the document that was never in anything's environment, so the line is
  unescaped, and an escape outside systemd's table is refused rather than guessed at.
- **`EnvironmentFile=` contributes nothing to this property.** A variable set only in the
  file did not appear on the line, because systemd reads those at exec time and not at load
  time. So this field answers "what does the unit declare", and *not* "what does the service
  run with". That gap is real and is the reason the file paths are worth collecting next.

**This is not the opposite of the `ExecStart` decision, and the difference is the point.**
[The argument vector is kept whole](../crates/rastro/src/collectors/systemd/exec_start.rs)
because systemd loses the quoting in `argv[]`, so
splitting it would claim a structure the source cannot support. `Environment=` keeps its
quoting, so the entries are recoverable exactly, and the honest record is the split one. A
reader who knows the first entry would otherwise assume the same limitation applies here.

## The files a unit reads, beside the variables it declares

`EnvironmentFiles=` lands in the same facet, and it is what stops the previous section being
a trap. A service configured entirely through one declares an empty `environment` and runs
with a full one; the two fields are only an answer read together, which both now say in
their own documentation.

**Paths only. What is inside the files is not read.** So the facet says which files a
migration must not leave behind, and does not say which variables would go missing if it
did. That is the smaller claim and it is the one the data supports.

**`ignore_errors` is systemd's word and is kept as systemd spells it**, for what a unit file
writes as the `-` prefix in `EnvironmentFile=-/path`. `required` was considered and rejected:
the double negative is unlovely, and the vocabulary of the tool being quoted is the spelling
a reader can look up. The distinction is behaviour and not bookkeeping — a required file
that did not survive a migration stops the unit, and an optional one is designed for exactly
that absence.

**The order is kept and never sorted.** systemd reads these in the order the unit declares
them and a later file overrides a variable an earlier one set, so the order *is* the
meaning. The same reasoning that keeps kernel order in `/proc/mounts`.

**Two spellings of absence in one dump, and the parser has to know both.** A unit that sets
no variables prints `Environment=` with nothing after it; a unit that names no file prints
no `EnvironmentFiles=` line at all. Measured, not assumed.

**Nothing here is withheld.** A path is not a credential, and it is the whole of the
migration finding. The values on the `Environment=` line still are.

**Cost:** the facet now names a file it cannot read, so a diff can show that
`/etc/myapp.env` is still there and still say nothing about the variable inside it that
changed. Reading those files is a separate decision, and it is the one where redaction
starts earning its keep on this facet rather than merely being available.

# Every collector is version `1` until rastro has a release

Dated 2026-09-08. rastro is at `0.0.0` and has never been released. Six collectors had
nonetheless moved past `1` — `firewall`, `network`, `processes`, `sockets` and `time` to
`2`, `postgresql` to `3` — each for a reason that was locally sound and collectively wrong.

**A collector version is a promise to somebody holding an older document.** Its whole job
is to let a consumer diffing two fingerprints tell "the collector's output shape moved"
apart from "the box changed". Before a release there is nobody in that position: no
document exists that was produced by a published rastro, so there is no archive for a bump
to protect. Each bump was priced as though the format were already published, and what it
bought instead was a version field whose meaning depended on when a collector happened to
last be touched.

**So the rule is flat: every collector reports `1` until the first release**, and the six
are reset to it. What a bump would have recorded is not lost — it is in this log, which is
where a pre-release format change belongs.

**After the first release the ordinary rule resumes**, and a facet that changes its key set
on identical host state bumps as those six entries described.

**Supersedes the version paragraph, and only that paragraph, in five entries.** The
decisions themselves stand entirely; each still describes a real change to what its facet
reports, and only the bump it prescribed is withdrawn:

- [The time collector reads files, because `timedatectl` starts a unit](#the-time-collector-reads-files-because-timedatectl-starts-a-unit)
- [`ip` is asked for details, because it hides a route's defaults](#ip-is-asked-for-details-because-it-hides-a-routes-defaults)
- [The sockets facet is read from `/proc`, and loses the interface scope](#the-sockets-facet-is-read-from-proc-and-loses-the-interface-scope)
- [A firewall backend is read only where its subsystem is already resident](#a-firewall-backend-is-read-only-where-its-subsystem-is-already-resident)
- [A role password change is visible, and is hashed twice to get there](#a-role-password-change-is-visible-and-is-hashed-twice-to-get-there)

**Cost:** a fingerprint taken from a build of rastro before this change compares against one
taken after with six facets whose version went *backwards*. That is only meaningful to
somebody holding a document from an unreleased build, which is the population this entry
argues does not need protecting, and it is the last moment at which the reset is free.

**Consistency check for a reviewer:** `grep -c 'CollectorVersion::new("1")'` over
`crates/rastro/src/collectors/` should equal the number of collectors, and nothing should
match `"2"` or `"3"`.

# The environment files are read, and that is the nginx exception a second time

Dated 2026-09-15. The previous section left the facet naming files it could not read. This
closes that, and it does so by parsing a configuration format, which needs the same
justification nginx needed.

**The licence, and why it applies.** The rule is to prefer effective, resolved state over
reading config, and `systemctl show -p Environment` *is* that effective state — for what a
unit declares. It deliberately does not cover these files: systemd opens them when it execs
the process, not when it loads the unit, measured against systemd 257 where a variable set
only in a file did not appear on the property. Every way of making systemd resolve them
starts the unit, which is a mutation, so there is no non-mutating account to prefer. That is
exactly the nginx condition, and the format is read directly.

**What bounds the risk of disagreeing with systemd's own parser.** Two things, and neither
is confidence.

- Every rule was **measured**, by pointing a unit with `ExecStart=/usr/bin/env` at a probe
  file under systemd 257 and reading the bytes back. `tests/environment_file_contents.rs`
  holds the probe whole, so a divergence shows up as a failing test rather than as a wrong
  value in somebody's fingerprint.
- **Values are `sensitive`.** A value this parser gets subtly wrong still digests
  deterministically and still diffs, so byte-identity and change detection are unaffected;
  only `--raw` would show the divergence. Names are *not* withheld, which is why the
  line-level rules matter more than the escapes, and the tests concentrate there — a
  mishandled continuation invents or loses a name, and a name is printed in the clear.

**Two rules that are the opposite of the unit-file syntax they look like.** A shared escape
table would get both wrong:

- `"a\nb"` in an environment file is a backslash and an `n`. On a unit's `Environment=` line
  it is a line feed.
- A backslash outside quotes is an escape and vanishes (`a\b` is `ab`); inside quotes it is
  kept unless it precedes `"`, `\` or the end of the line.

**A third rule, found later and by a failing test rather than by reading.** Trailing
whitespace is dropped only where it was *outside* quotes: `V="  sp  "` keeps both runs of
spaces, `V=val␠␠␠` loses its three, and `V="ab"cd␠␠` loses the two that follow the closing
quote. Trimming once at the end of the scan — the obvious implementation, and the one that
shipped first — gets the first case wrong and no measured example had caught it, because the
probe file quoted nothing that was padded. The parser now tracks the last significant
position as it scans. The lesson is the cheap one to record: a probe file proves the cases it
contains, and the gap between "measured" and "measured exhaustively" is where this bug lived.

**A repeated name takes the last value, where cron refuses one.** Not an inconsistency: a
crontab is genuinely ambiguous about what its jobs run with, so refusing is the honest
answer there. systemd's semantics here are defined, so mirroring them is reporting the box
and anything else would be rastro inventing a disagreement.

**A line systemd sets nothing from is counted, not dropped.** `export FOO=bar` is what an
operator writes out of shell habit; the name would contain a space, so systemd sets nothing
and the file still looks right. `ignored_lines` makes that visible without this parser
guessing at what was meant. It is the one field here that reports a *mistake* rather than a
state.

**Three readings, using the facet's own vocabulary one level down.** `ok`, `absent`,
`error`. A missing file is state — routine for one marked `ignore_errors`, and the whole
finding for one that is not, since that unit will not start. A file that is there and will
not open is an `error` carrying its reason. `NotFound` is the only errno read as absence,
because reporting `absent` for a file rastro was merely not allowed to read would be a
confident lie, and rastro is run unprivileged often enough for that to be routine.

**One file's failure never fails the facet.** The same call the nginx collector makes for an
include that will not read. Losing the enablement state of every unit on the box because one
service's environment file is root-only is the worse trade by a distance.

**What a box without systemd gets from this: nothing.** Verified on Alpine, where there is
no `systemctl` and the `units` facet is `absent` — correctly, since it is a true statement
about that box, and the run is otherwise unaffected. It follows from environment belonging
to its carrier rather than to a facet of its own, and it is the direct cost of that choice:
rastro's environment coverage is exactly as wide as the carriers it knows. Today that is
systemd units and crontabs, so a non-systemd box reports crontab variables and no others.

Two things follow, and the first is the more urgent. `/etc/environment` and
`pam_env.conf` are init-agnostic, so they are the floor under every box rather than a
rounding error on a systemd one — which is the argument for collecting them and is stronger
than the one originally made for it. And OpenRC's `/etc/conf.d` and sysvinit's
`/etc/default` are uncollected: each would be a new carrier reporting its own environment,
in the shape `units` now has, which is the test the Debian-first convention sets and this
design passes without a breaking change.

**Cost:** rastro now opens files named by unit configuration, which is a wider read than the
facet had before, and it does it on every run. The files are small by construction — systemd
reads them itself at every service start — so this is a cost in *surface* rather than in
time: a unit file that names a path is now a path rastro will open.

# What a review round found in the environment work

Dated 2026-09-16. Seven findings against the three entries above. Recorded together because
what they have in common is the lesson: **every one of them was a behaviour of systemd that
the probe file did not contain.** The rules were measured, and measuring is not the same as
measuring exhaustively.

**`EnvironmentFile=` takes a wildcard, and `systemctl show` reports it unexpanded.** Measured:
`EnvironmentFile=/etc/conf.d/*.env` comes back from the property with the `*` intact, so
reading the declaration as a path found nothing and reported the whole set `absent` — losing
exactly the variables the facet exists to name. Patterns are now expanded in byte order,
which is the order systemd applies them in: a variable set in two matched files takes the
later one's value. A pattern that matches nothing keeps one entry with a null `path`, because
a *required* wildcard matching nothing stops the unit from starting, measured as
`Result=resources`.

That made the declaration and the file two different facts, so the entry carries both:
`declared` changes when somebody edits the unit, `path` changes when somebody drops a file
into the directory, and a facet that conflated them could not say which happened.
`file_glob` moved out of the nginx collector to sit beside `canonical_tool`, since two
collectors now meet the same `glob(3)` question.

**`UnsetEnvironment=` is the last step of building a service's environment.** Measured: a
unit with `Environment=TOKEN=secret` and `UnsetEnvironment=TOKEN` starts a process with no
`TOKEN`. Without the property the facet claimed a service had a variable it never receives,
which is the one thing a fingerprint must not do. Reported beside the declarations rather
than applied to them: deleting the `Environment=` line and adding an `UnsetEnvironment=`
reach the same process environment by different edits, and only a document carrying both can
say which one happened.

**Three corrections to the environment-file grammar**, each measured:

- **The backslash before `$` and a backtick is stripped.** `"cost\$5"` reaches the process as
  `cost$5`. The table had only `"` and `\`, so the document recorded a character the process
  does not have — and the wrong redaction digest with it.
- **A name is a C identifier.** `BADNAME-X=x` and `1BAD=y` set nothing at all; `OK_NAME=z`
  arrives. Rejecting only whitespace let the facet claim variables the service does not have.
  Checked in this parser rather than in `EnvironmentVariableName`, which is shared with cron
  and deliberately imposes no character rule: the grammar is systemd's, so it belongs with
  systemd's reader.
- **What continues a line is the parity of the trailing backslash run**, not the last
  character. `V=a\\` then `W=b` sets both; `V=a\` then `W=b` sets the one variable `V=aW=b`.
  Treating an even run as a continuation swallowed the following line, so an assignment the
  service really has vanished from the document with nothing to say it had been dropped. That
  is the worst of the three, because the others mis-read a value and this one loses a name.

**One finding was half right, and the half that was wrong is worth recording.** The review
also held that a quoted value spans physical lines without a continuation. It does not:
`MULTI='first` / `second'` gives systemd `MULTI=first` and sets nothing from the orphaned
line, exactly as this parser already did. Checked rather than accepted, and there is now a
test pinning it, so the next reader does not re-open it.

**The declared path is not assumed to be a file.** A unit may name a FIFO, a character device
such as `/dev/zero`, or something far larger than any environment file, and `read_to_string`
on any of those blocks or exhausts memory — one such declaration anywhere on the box would
stop the whole run. The type is checked by `stat` *before* anything is opened, because
opening a FIFO blocks until a writer appears and a check made afterwards never runs. The read
is then bounded twice, once on the size stat and once one byte past the limit, the second
because a file being appended to between the two is exactly what a bound is for. The same
reasoning as the execution seam's output bound.

**Reusing `file_glob` found a latent bug in it, and the review did not.** `is_pattern` tested
only for `*` and `?`, so a declaration whose only metacharacter is a bracket — `[ab].env` —
was read as a literal path and reported `absent`. That is the same silent wrongness as not
expanding `*`, and harder to spot, because the path looks ordinary rather than obviously
unexpanded. It answers `true` now, which routes the declaration to the refusal the module
already had. nginx was wrong in the same way and is fixed by the same line, which the
conformance test — rastro's include resolution against `nginx -T`'s own answer — confirms on
both distributions.

**Cost:** an environment file past a megabyte is now reported as an `error` rather than read.
That is a misconfiguration by construction — systemd reads these itself at every service
start — but it is a case rastro now declines rather than one it answers.

# One spelling for an environment variable name, across three carriers

Dated 2026-09-16. The `containers` collector landed on master while this work was in flight,
carrying its own `VariableName` for the same concept. Two spellings of one thing inside one
document is exactly what the ubiquitous-language rule forbids, so `containers` now uses
`rastro-collector`'s `EnvironmentVariableName` and its own type is gone.

**The port's rule decided it rather than taste:** a value earns its place there by having
consumers in more than one collector. `cron` had one, `units` made two, `containers` makes
three. A crontab's `PATH`, a unit's `Environment=PATH` and a container's `PATH` are one
concept observed on three carriers, and a reader diffing a document has to be able to compare
them.

**The merge went the stricter way on exactly one character.** `containers` refused `=` in a
name and the shared type did not, so the shared type now does. That is not the stricter rule
winning by default — it is the only character rule that is about the *format* rather than
about taste: `=` separates the name from the value, so a name holding one means the entry was
split in the wrong place and whatever landed either side of it is untrustworthy. Everything
else stays permissive, because `execve(2)` carries any byte but `=` and NUL and a name that is
really on the box must be reportable.

**Where a stricter grammar does belong: with the parser that needs it.** systemd sets nothing
from a name that is not a C identifier, so the environment-file reader drops `1BAD=y` and
counts it — while a container engine reporting that same name is reporting something the
process really has. Putting systemd's grammar in the shared type would have made two other
facets lie. This is the general shape: **the shared type carries what is true of the concept,
and a collector enforces what is true of its own source.**

## What this settles about boundaries, and what it does not

This is the first case where two collectors genuinely collided over one concept rather than
over one *fact*, and it is worth separating the two, because an earlier round of this work
deferred writing a general boundary rule on the grounds that the evidence was all one pattern.

- **Two collectors observing the same concept share a type.** Settled here.
- **Two collectors observing the same fact from different sources keep both**, because they
  can disagree and the disagreement is the finding. Already settled three times: a configured
  endpoint against a bound socket, a configured port against `postmaster.pid`, a unit's
  `ExecStart=` against the process table.
- **Two collectors reading the same source share the reader**, not the facet: `units` and
  `exporters` over one `systemctl show` dump, and now `nginx` and `units` over one
  `file_glob`.

Those are three different rules and they were being conflated. A general "boundary rule"
entry is still not written, and still should not be until something arrives that none of the
three settles.

# PAM gets a collector, because PAM is what reads those files

Dated 2026-09-16. `/etc/environment` and `/etc/security/pam_env.conf` were going to be an
extension of `accounts`. They are not, and the reason the earlier plan was wrong is worth
recording, because it was a misapplication of a rule rather than a bad rule.

**Environment belongs to its carrier.** That is what put a unit's `Environment=` in `units`
and a crontab's variables in `cron`. `pam_env.so` is what reads these two files, at session
setup — not the shell, and not anything that reads `/etc/passwd`. `accounts` was the nearest
neighbour, not the owner, and putting them there would have broken the very rule that
justified the other two placements.

So the gap was never "environment has no home". It was **"PAM has no collector"**, which is a
hole in Layer 2 independent of environment: the `pam.d` stack and `limits.conf` are real
system state nothing reports. This collector is scoped to the session environment for now and
those are its obvious next tenants, needing no new facet.

**An `environment` collector was the alternative and is rejected.** It would be defined by
exclusion — the environment sources no other collector owns — and a residual category shrinks
and shifts every time another carrier gets a collector. Its name would also promise more than
it delivers: holding neither unit nor cron environment, "the environment collector" would be
the one place in the document without most of the environment.

## `/etc/environment` is not the systemd format, and three rules are its opposite

Measured against `libpam-modules` 1.7.0 on Debian 13 — a probe file written, a PAM login
performed, the resulting environment read back. The file looks exactly like a unit's
`EnvironmentFile=` and is read by a different program:

| line | `pam_env` | systemd |
|---|---|---|
| `export V=x` | sets `V` | sets nothing; the name holds a space |
| `V=x␠␠␠` | keeps the spaces | trims them |
| `V=a # b` | truncates at the `#` | `# b` is part of the value |

Two more that a shared parser would also get wrong: quoting does **not** protect a `#`
(`V="a # b"` is `a `), and only one leading quote is stripped with a trailing one taken off
solely when it ends the value, so `V="ab"␠␠` really is `ab"␠␠`. No escape is processed at all.
So there are two parsers, and what they share is the vocabulary rather than the code: both key
their result by `EnvironmentVariableName`.

**What `pam_env.conf` records is the rule and not the result.** A value may hold `@{HOME}` or
`${USER}`, expanded per session and per account at login — measured, `DEFAULT=@{HOME}/x`
reaches one account as `/home/probe/x`. Recording a resolved value would mean naming one
account's answer as though it were every account's, and resolving it honestly would mean
performing a login, which is the mutation this project refuses. `DEFAULT` and `OVERRIDE` are
both kept for the same reason: `OVERRIDE` wins, and a facet that reported only the winner
could not show that removing it would change the value.

**Presence is two-valued on `/etc/pam.d`.** A box without it does not run PAM, so there is no
session environment for PAM to set. A box with no PAM may still have an `/etc/environment`,
and this facet stays silent about it deliberately: that file would then be read by something
else, and reporting it here would claim PAM does something it does not.

**Cost, and it is the one to know about:** `envfile=` sources named in the PAM stack are not
discovered. Debian's `/etc/pam.d/su` carries a second `pam_env.so` line reading
`/etc/default/locale`, which is where `LANG` actually comes from on most Debian boxes. Finding
it means parsing the `pam.d` stack — the collector's next step rather than this one — so the
facet reports the two sources `pam_env` reads by default and says so rather than implying it
has the lot.

---

# A tree more than one claim names

Dated 2026-09-17. From issue #41: a `filesystem` facet that came back `error` while every
other facet was `ok`, and `invocation.data.walk_policy` null, because the run stopped while
building the table. The document was written and the command succeeded. The only outward sign
was that it was a quarter of its usual size.

## Two clusters registered on one data directory is a state a box really reaches

Measured on the reporter's own document rather than guessed. `14/main` is online and its
`data_directory` is `/var/lib/postgresql/data`, set at `/etc/postgresql/14/main/postgresql.conf`
line 45. `11/main` is `down,binaries_missing`, which is the shape a purged old-version package
leaves, and its surviving config names the same directory. Both are registered, so
`pg_lsclusters` prints that directory on two rows and the collector resolves a claim per row.

Impossible as a *running* state, because `postmaster.pid` lets only one server hold a
directory. Entirely possible as a *registration*, and an upgrade nobody finished with
`pg_dropcluster` is how a box gets there.

**Nothing about it can be resolved from the outside.** A cluster that is down while the walk
runs may own the directory and be about to come back up, so "the running one owns it" is a
guess, not a reading. rastro is looking at two registrations that cannot both be right and has
no way to tell which one is wrong.

## A cluster's registered data directory is in the facet

Read from `pg_lsclusters` rather than from `pg_settings`, and that is the whole point: a
stopped cluster has nothing running to ask. `11/main` would otherwise say nothing at all about
where it points, and the collision would be invisible in the document even after the walk
stopped failing over it.

Two clusters on one directory is now a fact the facet states, beside the trees the walk sealed
and the entry that says who argued. The collector does not deduplicate its own claims to make
the fold succeed: what it read is what it reports.

## A claim names which of its claimant's entries asked

A claim carried a tree and a reading, and the claimant was supplied by whoever gathered the
claims, because a collector naming itself could name somebody else. That left `postgresql`
claiming one tree twice, which reads as a bug in rastro rather than as the misconfiguration it
is.

So a claim may carry a **qualifier**: the key the asking entry has in its own facet's `data`.
The gatherer still supplies the facet half, so the property that mattered is untouched — a
collector can mislabel its own entry and can never file a decision under a peer's name. The
composed claimant is `postgresql:14/main`, and the colon is reserved in a qualifier so the
composed name can always be read back. A facet with one subject has nothing to qualify and
gains no punctuation.

## A tree more than one claim names is sealed, and every claimant is kept

Supersedes [A tree two collectors claim fails the `filesystem` facet](#a-tree-two-collectors-claim-fails-the-filesystem-facet).
The reasoning there was right about the resolution and wrong about the price: there is no way
to pick a winner, and the answer to that was to fail the largest facet in the document.

**Sealing is not rastro settling the argument.** It is rastro declining to walk into a tree it
cannot account for, which is the one answer that needs no winner. What the claims asked for
stops applying, and that includes claims that agreed: two clusters registered on one directory
both say `sealed` and are still a box in a state nobody intended. Agreement between claimants
is not evidence that anything is well.

**No special cases.** rastro's own shipped rules are claimants like any other, so a collector
that duplicates one contests it. The root is a tree like any other: a carve-out there would
buy a branch and nothing else, because a sealed root still leaves one entry saying what
happened, which is exactly what the `/`-stat carve-out exists to prevent and this case does
not need.

**Never a dead end.** An operator's rule replaces whatever it names, a contested seal
included, and the tree stops being contested rather than staying sealed with a note. The
operator knows their box, which is the thing rastro was missing.

**Cost, and it is accepted rather than argued away:** a collector pair that both resolve one
tree correctly now costs that subtree until somebody fixes one of them. That is either a bug
in rastro or a misconfigured box; both want fixing, and both are worth a subtree that is loud
about being missing.

## The contest is reported at the entry, not at the facet

The third rung of a ladder this log already built: the run, then
[the facet](#a-tree-two-collectors-claim-fails-the-filesystem-facet), then
[the entry](#a-path-that-is-gone-is-omitted-a-path-that-will-not-be-read-is-recorded).
A contested tree is the entry.

The `filesystem` facet is keyed by path, and a path rastro could not read is already rendered
at its own key as a reason rather than a description. A contested tree joins it, on the same
contract: an entry is its attributes or the reason it has none. An ordinary seal keeps the
directory's own mode and owner; a contested one does not, and that difference is what stops a
reader skimming past it as a normal seal.

The sentence is the rule's own, so the line the operator sees on stderr while the run happens
and the reason in the document cannot drift apart. `claimed_by` in the effective table is a
list at every rule rather than a scalar that becomes a list on the boxes where something went
wrong: a shape that changes with the data is a shape every reader has to branch on.

**What the reporter's box would now produce:** the `filesystem` facet `ok`, one entry at
`/var/lib/postgresql/data` reading `claimed by postgresql:11/main, postgresql:14/main, so it
was sealed rather than walked`, both clusters in the `postgresql` facet naming that directory,
and every other path on the box still in the document.

# A build names the commit it came from, in the version it reports

Dated 2026-09-17. The `invocation` facet's job is to say what produced a document, and it
could not. Every development build reports `0.0.0`, the workspace version, because both
sites that report one read `CARGO_PKG_VERSION` at compile time. Two fingerprints taken by
two different builds of rastro are indistinguishable in the field that exists to tell them
apart, which was found the honest way: a pair of real fingerprints turned out to carry a
collector set matching no commit in `master`, and nothing in either document could say which
build had made them.

**So a build may override the version, and the rolling build does.** `RASTRO_BUILD_VERSION`,
when set at compile time, replaces the crate version:

```
$ rastro --version
rastro 0.0.0-rolling+75068c4
```

**Semver, deliberately, and the identifier is `rolling` because the tag is.** The base is the
crate version read from the manifest, so cutting a release needs no edit in the workflow.
`-rolling` is a pre-release identifier and `+<commit>` is build metadata, which semver
ignores for precedence — correct here, because the commit identifies a build rather than
ordering it. The name matches the `rolling` pre-release the binary is published as, rather
than introducing a second word for one artefact. `nightly` was considered and is wrong twice
over: the build is published on every master push rather than daily, and the tag name is
burned on this repository, as
[the distribution entry](#distribution-getting-the-binary-before-there-is-a-release)
records.

**The trap this leaves for the first release.** `-rolling` is a pre-release, so
`0.1.0-rolling+abc` sorts *below* a released `0.1.0` while actually being ahead of it. The
answer is the ordinary one: bump the workspace version immediately after cutting a release,
so master always carries the next unreleased number and a rolling build off it sorts above
what shipped. Free to say now, expensive to discover later.

**One constant, because two were one accident from disagreeing.** The command line and the
`invocation` facet both report a version. They agreed because both read `CARGO_PKG_VERSION`;
once a build can override it, agreement has to be built rather than assumed, so both read
`rastro::VERSION` and a test asserts the printed string contains the documented one.

**A build script, for two lines.** `option_env!` is read when the crate is compiled and cargo
has no way to know it was consulted, so without `cargo::rerun-if-env-changed` a later build
with a different commit silently keeps the old string. CI builds clean and would never
notice; somebody reproducing a rolling build locally would, and would be debugging the wrong
thing. Measured both ways before it was written down: with the build script, changing the
variable changes the binary; the fallback to `0.0.0` still holds when it is unset.

**CI asserts the commit arrived**, rather than running `--version` as a smoke test. A build
script and an environment variable can both fail silently, and a version that quietly stayed
`0.0.0` would reintroduce exactly the gap this entry closes.

**Cost:** a fingerprint's `rastro_version` now differs between two builds of identical source
from different commits, so a diff of two documents taken by two rolling builds shows one line
in the `invocation` facet on an unchanged host. That is the intended reading — the builds
really were different — and it is the same cost the disclosure entry accepted for the same
facet.

# A database's grants are keyed by grantee, and the grantor stays a field

Dated 2026-09-17. The `postgresql` facet rendered each database's ACL as a list of grant
objects, each carrying its `grantee`, its `granted_by` and its privileges. Every other
collection in that facet is keyed: `databases` by name, `roles` by name, `memberships` by
member and then by granted role, `extensions` by name. Grants were the exception, and a real
pair of fingerprints showed what the exception costs.

**The case, and it is measured rather than imagined.** A secrets-management cutover on a
live box moved two databases off their migration roles onto `postgres` with
`ALTER DATABASE … OWNER`. That statement does three things at once: it rewrites `granted_by`
on every existing entry, it gives the new owner an explicit entry the ACL did not carry, and
it takes the implicit owner rights off the old one. Three renderings of the same two real
fingerprints, each flattened to leaf paths and diffed the same way, over one of those
databases. The counts are that database's; the role is renamed here to the vocabulary the
test fixtures use, because the box it came from is not this project's to name:

| shape | diff lines | where the revoke lands |
|---|---|---|
| a list of grants | 41 | `grants[7]/privileges/…`, an index naming nobody |
| keyed by grantee **and grantor** | 73 | **nowhere: the grant is removed and re-added** |
| keyed by grantee, grantor a field | 32 | `grants/migrator[0]/privileges/…` |

**The list smears an insertion.** `postgres` gaining an entry at index nine shifted the tail
along, so four grants reported a changed `grantee` and a fifth appeared whole, and the two
lines that were the point sat at index seven among the artefacts. A reader skimming that sees
rename noise and stops, which is what happened.

**Keying on the grantor as well is worse, and that is the finding worth keeping.** It is the
obvious shape, because a grant really is identified by the pair, and it was built and
measured before it was rejected. The grantor rewrite renames every key at once: each grant
reads as one key removed and one key added, the diff never descends into either, and the
revoke the same statement performed is not distinguishable from the rename anywhere in the
output. A shape that hides the change it was adopted to reveal is worse than the one it
replaced, and only running it against the real pair said so.

**So the key is the grantee and the grantor is a field.** The insertion is one key appearing.
The rewrite is one field per holder, stated once each instead of once per row. The revoke is
two keys leaving an object at a path naming the role they were taken from.

**A grantee holds a list, because a grantee is not a unique key.** The same role can hold
`CONNECT` from one grantor and `CREATE` from another; Postgres keeps them as separate
aclitems and a `REVOKE` has to name the grantor. The list is one element wide in every
ordinary ACL, and a position in it only shifts within one grantee's own grants rather than
across the whole database's, which is the smearing this entry exists to stop.

**What is given up, and it is small but real.** `Grantee::Public` ordered first by
construction, so `PUBLIC` headed a database's grants whatever it was called — the grant every
login role's `CONNECT` actually rests on, at the top where a reader looks. Object keys are
sorted as strings by the document's own structure, so `PUBLIC` now heads a database's grants
only because it is uppercase and Postgres folds an unquoted identifier to lowercase. A role
deliberately created as `"ANALYST"` would sort above it. That is a rendering order rather
than a claim about privilege, and paying for it with a hand-maintained key order would put
ordering back in collector discipline, which the document's shape exists to keep out.

**No collector version bump**, per
[the release rule](#every-collector-is-version-1-until-rastro-has-a-release): `postgresql`
stays at `1` like everything else until rastro has a release. This entry is where the format
change is recorded until then.

**Cost:** a fingerprint taken before this change cannot be diffed against one taken after for
the `grants` of any database, because the shape under that key is different. That is the
whole population of documents produced by an unreleased build, and the change is worth more
than they are.

**What this does not fix.** The facet reads `datacl` and nothing else: no `nspacl`, no
`relacl`, no `pg_default_acl`. If that same cutover had re-owned a schema or re-granted a
table, no shape of rendering would have shown it, because the collector never asked. That is
a gap in coverage rather than in presentation, and it stays open.

# A socket's holders are keyed by the program name, not listed per process

Dated 2026-09-20. From issue #37: the `sockets` facet rendered each socket's `processes` as
a flat list of `(name, pid, file_descriptor)` triples, with the pid and the descriptor
annotated volatile. Both annotations are right: a pid changes every time a service restarts
and the descriptor number changes with it. What reached the diffable view was therefore a
list of bare names, and its **length** was a property of the moment `/proc` was scanned
rather than of the host.

**The failure, measured.** The determinism harness caught it in a container where dockerd
had just started and forked a child that inherited the listening descriptors. One run
carried `/var/run/docker.sock` with two identical `{"name":"dockerd"}` entries, the next
carried one, and two further sockets of the same daemon differed the same way. Two runs of
an unchanged host stopped being byte-identical, which is the contract every other facet
rests on and the reason there is no diff verb. Nothing about it is particular to docker:
any forking daemon does it, and nginx's master and its workers are the next case.

**The fix is the cardinality, not the annotation.** A socket is held open by one or more
*programs*, and each of those has one or more live processes behind it. The old shape
flattened the two levels into one, so the volatile half could not fall away without taking
the list's shape with it. The facet now carries `holders`, a set keyed by the name, and
each holder carries its own `processes` set of pid-and-descriptor pairs annotated volatile
*whole*. The diffable view is then one object per distinct name, whatever is underneath it,
and `--include-volatile` still answers which processes those are for an operator standing
in front of the box.

**Rejected: a stable list of names beside the volatile triples**, which is the direction the
issue sketched. Two lists that have to agree, derived from one reading, and the complete
view would carry both — so a reader diffing the complete view meets the duplicate names
again. Grouping removes the duplication from both views instead of hiding it in one.

**Rejected: dropping the pid and the descriptor.** It would fix the same thing by recording
less, and the pair is what makes the complete view worth asking for: `ss -p` exists because
an operator chasing a port wants the pid.

**No collector version bump**, per
[the release rule](#every-collector-is-version-1-until-rastro-has-a-release).

**Cost:** a fingerprint taken before this change cannot be diffed against one taken after on
the `sockets` facet, because the key is `holders` rather than `processes` and the shape under
it is different. That is the whole population of documents produced by an unreleased build.

**What this does not fix.** The name is `/proc/<pid>/comm`: truncated to 15 characters and
settable by the process itself, so two genuinely unrelated programs sharing a name share a
holder entry. That was equally true of the old shape, which listed them as two entries
without being able to say they were different. Grouping makes the ambiguity one entry rather
than two, and neither shape can resolve it, because the kernel does not offer the fact.
# RabbitMQ: what a read of a broker costs, measured before the collector

_2026-09._ The `rabbitmq` collector is not built. These entries are the measurements that
shape it, taken before a line of it was written, because a Layer 3 collector for this service
has to answer one question before any other: whether reading the thing is allowed at all. A
RabbitMQ CLI tool is not a client that opens a socket and asks. It boots an Erlang VM,
registers with the port mapper daemon and joins the broker's own distribution cluster, which
makes it the most invasive read this codebase has considered.

Measured on Debian 13, RabbitMQ 4.0.5, Erlang 27 (erts 15.2.7), aarch64, in a container and
therefore without systemd. The recipe: install `rabbitmq-server`, leave it stopped, invoke,
then compare a marker file against every path on the filesystem. The epmd findings are the
kind that hold across versions; the output shapes are the kind that do not.

## A CLI invocation starts epmd, so nothing is asked speculatively

| invocation | exit | wall | left behind |
| --- | --- | --- | --- |
| `epmd -names`, with no epmd running | 1 | 2 ms | nothing |
| `rabbitmqctl status` as root, node down | 69 | 342 ms | `epmd -daemon` |
| `rabbitmqctl status` as `rabbitmq`, node down | 69 | 351 ms | `epmd -daemon` |

The failing invocations are the finding. Each achieved nothing, reported that the node was
unreachable, and left `/usr/lib/erlang/erts-15.2.7/bin/epmd -daemon` running on a box that had
no such process a moment earlier. epmd outlives the invocation by design: it double forks and
detaches, which is how the broker's own start script gets one.

This is [the invariant](#rastro-does-not-change-the-host-it-describes) and the failure mode
[the `timedatectl` reversal](#the-time-collector-reads-files-because-timedatectl-starts-a-unit)
already named: starting a daemon is a mutation however small, and a before-and-after pair an
operator takes around a change would carry rastro's own footprint as part of the change.

**So the collector never invokes a CLI tool to find out whether it can.** epmd has to be
resident already, established by reading the process list, which is
[the rule the firewall backends follow](#a-firewall-backend-is-read-only-where-its-subsystem-is-already-resident):
a subsystem-specific tool runs only where its subsystem is up. A box with RabbitMQ installed
and nothing running is `present` with a node reported down, from files and `/proc`, and no
invocation at all.

**Cost:** a node that is somehow up while epmd is not would be reported as down. A
distributed Erlang node registers with epmd as part of coming up, so this is a state the
runtime does not produce, and the direction to be wrong in is the one that changes nothing.

## epmd is the register, because a node cannot be named from `/proc`

The plan for this collector had `/proc` as the register, on the reasoning that a pure read
cannot start anything. The reasoning was right and the premise was wrong:

- the beam's `environ` holds **zero variables**, not merely no `RABBITMQ_*` ones;
- its `argv` carries `-home /var/lib/rabbitmq` and `-s rabbit boot` and **no node name**.

So `/proc` can say a RabbitMQ node is running and cannot say what it is called, which is the
one thing a CLI invocation must be told.

**epmd answers exactly that, and the probe is free.** `epmd -names` prints a line per
registered node with its distribution port, works for an unprivileged caller, and, measured
above, does not start the daemon it fails to reach. A CLI call also deregisters its own hidden
node cleanly: the register is byte-identical before and after one.

```
epmd: up and running on port 4369 with data:
name rabbit at port 25672
```

The dispatch is therefore three steps, none of which may be reordered: epmd resident in the
process list, then `epmd -names` for the node names, then the CLI addressed at a named node.

**What `/proc` keeps** is the job it can do: a beam whose `argv` names `rabbit` is evidence a
broker rather than some other Erlang application is what epmd registered.

**Unresolved, and not to be depended on until it is measured on a real host.** Resolving a
distribution port back to its holding process failed in the container: epmd named port 25672,
`/proc/net/tcp` gave the socket inode, and no descriptor of any beam matched it. In the same
reading `readlink` of that process's `exe` and `cwd` returned nothing as root, so the
container's `/proc` is the likelier culprit than the method, which
[the sockets facet](#the-sockets-facet-is-read-from-proc-and-loses-the-interface-scope) relies
on and exercises elsewhere. Nothing in the dispatch above needs it.

## A read of a live broker touches nothing, and an idle window is why that means anything

Nineteen read commands against a running node: `status`, `cluster_status`, `environment`,
`export_definitions -`, eleven `list_*` commands, four `rabbitmq-diagnostics` reads and
`rabbitmq-plugins list`. Between a marker file set before them and the check after, **no path
on the filesystem moved**, and the broker's log file digested identically before and after.

That zero means nothing on its own, which is the point of the control: watched for ten idle
seconds with no read in it, the same broker touched three `.dets` files under its data
directory and its own log. A store that writes to itself is the background any read of it is
measured against, and
[clearing a tool by inspection](#rastro-does-not-change-the-host-it-describes) is how two
collectors in this codebase were wrongly cleared already.

Wall clock per invocation, which is an Erlang VM boot each time: 243 ms to 458 ms, about five
seconds for all nineteen. `/proc/modules` was unchanged across the whole run.

**What this buys the design:** the expensive-looking read is the cheap one. It argues for few
fat commands over many thin ones, `export_definitions` carrying most of the facet in a single
254 ms call, and it says the hardened seam's existing bounds are adequate without a special
case.

## The node is asked by name, as root or as the broker's own user

`rabbitmqctl -n rabbit@<short-host> list_users` answers, and the node name comes from epmd
while the host half comes from the box. Who may ask:

| caller | outcome |
| --- | --- |
| root, holding no cookie of its own | exit 0 |
| the `rabbitmq` user | exit 0 |
| an unprivileged user with no cookie | exit 1, and a usage dump |

Root answered while holding no cookie of its own, which is measured; *why* it answered is
not. The package's `/var/lib/rabbitmq/.erlang.cookie` is mode `400 rabbitmq:rabbitmq` and root
reads through a mode, so that is the likely route and it was not confirmed. The collector
therefore does not rely on it: where root is refused, the broker's own user is reached through
the `ToolAsUser` seam the `postgresql` collector already runs `psql` under.

**No cookie is ever created.** `/root/.erlang.cookie` stayed absent through every invocation,
including the root call that had none to use, so the Erlang VM's habit of generating one when
it starts distribution is not reachable this way. It was the second thing this spike was built
to catch, and it is a measurement rather than a promise: the cookie file itself is
**described and never read**, the way
[nginx describes a private key it will not open](#the-certificate-is-read-the-key-is-only-described).

**The unprivileged failure prints a usage dump rather than a reason**, so the facet supplies
its own: an `error` naming root or the broker's user as the requirement. Passing a tool's
misleading text through as rastro's reason would make the document's own failures unreadable.

## The password hash is carried and withheld, which `postgresql` cannot do

`export_definitions` prints a user as a name, its tags, its `hashing_algorithm` and a base64
`password_hash`. The material arrives in rastro's process whether it is wanted or not, which
is precisely the property
[the role verifier entry](#a-role-password-change-is-visible-and-is-hashed-twice-to-get-there)
secured by hashing on the server and never reading the verifier at all.

**That structural guarantee is unavailable here, and the render-time mechanism is available
instead.** The hash is carried as a value marked `sensitive`, so the default document renders
`redacted:sha256+xxh3:<digest>` and `--raw` prints the hash. No second field beside it: the
redaction recipe *is* the postgresql digest, sha256 of the material then
[the port's one digest spelling](#one-digest-spelling-lives-in-the-port) over the hex, so a
collector-side digest would duplicate the stand-in while being unable to opt out of itself.

This facet is therefore the counter-example to the gap recorded in
[`--raw`, and a document that admits which one it is](#--raw-and-a-document-that-admits-which-one-it-is):
the verifier is the one value `--raw` cannot cover for `postgresql`, because opting out there
means asking the server a different question. Here it is an ordinary annotation, and the
difference is the source rather than the policy.

**Fail closed on the algorithm, and the salt is not the discriminator here.** The
PostgreSQL entry turns on SCRAM having a random salt where md5 has none. RabbitMQ has no such
split: its documented algorithm is the same for all three schemes, a random **32-bit** salt
prepended to the password, hashed, the salt prepended again, base64 encoded. So every scheme
is salted and the question the entry has to answer is a different one.

What differs is the cost of testing one candidate password against what the document carries.
The stand-in hides the salt, so a guess has to be tried against all 2^32 of them: four billion
hashes per candidate. Under SHA-256 and SHA-512 that is a real per-candidate cost. Under MD5
it is seconds of ordinary GPU time, which makes the stand-in for an md5 verifier a fast
offline oracle over any guessable password, and no further hashing by rastro repairs it,
because everything needed to recompute it is published beside it.

**So the verifier is carried under SHA-256 and SHA-512 and withheld under anything else**,
md5 included, and a scheme a later RabbitMQ adds included, until somebody has read how it
works. `rabbit_password_hashing_sha256` is what the measured broker reported for both its
users. A withheld verifier is `null` with the scheme beside it saying which case it is, and
the rule lives in one `match` so that adding a scheme makes the compiler ask the question at
the only site that answers it.

**A 32-bit salt is weak, and stating it is part of the decision.** Even carried, this
facet's stand-in is a weaker protection than the PostgreSQL one, whose SCRAM verifier brings
a large random salt and an iteration count. The honest summary is that the stand-in proves a
rotation happened and is not a vault; `--raw` is what the operator uses when they want the
value, and the security policy already says redaction is an option rather than a guarantee.

**Cost, and it is a real weakening.** Credential material lives in rastro's heap from parse to
render, which the postgresql design was built to avoid. Two guards follow, and both are
testable rather than disciplinary: no error path or debug rendering may carry a user record,
and a test asserts the fixture's hash appears nowhere in the default document and does appear
in the `--raw` one.

## `status` is half volatile, and `environment` is Erlang-shaped

`rabbitmqctl status --formatter json` is the node's own account of itself and mixes two kinds
of value in one object:

- **state:** `rabbitmq_version`, `erlang_version`, `config_files`, `log_files`,
  `data_directory`, `raft_data_directory`, `enabled_plugin_file`, `active_plugins`,
  `listeners`, `net_ticktime`, the memory watermark settings, `is_under_maintenance`;
- **moving on its own:** `memory` whole, `uptime`, `pid`, `run_queue`, `processes.used`,
  `file_descriptors`, `disk_free`, `totals.connection_count`.

The second list is annotated `volatile` and leaves the diffable view, which is
[the rule a moving catalogue already gets](#only-the-stable-columns-of-a-moving-catalogue-are-read).

**`rabbitmqctl environment` is the truest effective read and the most treacherous shape.** It
is small, 6.7 KB on a stock node, and its JSON is a projection of Erlang terms that does not
round trip: a string arrives as a list of character codes, so `mnesia.dir` reads
`[47,118,97,114,...]`, and a tuple arrives as an array. Nothing may be read out of it by
shape; every key taken from it needs a type somebody checked. So the effective read is
`status` plus the targeted `rabbitmq-diagnostics` commands, and `environment` contributes only
named keys.

**A stock Debian install has no `rabbitmq.conf` and no `enabled_plugins`, only
`rabbitmq-env.conf`**, and `status` reports `config_files: []` accordingly. On most boxes the
service's own account is not merely preferable to reading its configuration, it is the only
account there is.

## The node's data directory is sealed, and ten idle seconds are the argument

`/var/lib/rabbitmq/mnesia/<node>`, resolved from `status.data_directory` rather than assumed,
and sealed: the walk records the directory and does not descend.

The measurement is the idle control window above. With no client connected, no message
published and nothing asked of the node, three files under that tree moved in ten seconds.
Every attribute the walk would record of them moves again on the next write, which is noise in
a fingerprint whose whole claim is that two runs of an unchanged host are byte-identical. What
is actually in there, the vhosts, the users, the policies, the durable topology, this facet
reports properly from the node.

Same reasoning as [the trees nginx writes into](#the-trees-nginx-writes-into-are-sealed) and
the cluster directory `postgresql` seals, and the same qualifier applies: one claim per node,
each naming the node that asked, so a directory two of them point at says which two.

**A stopped node makes no claim**, because `status` cannot be asked and the default path is a
guess. The walk's own default is the safe direction to be wrong in, and the effective table in
the `invocation` facet says what happened.

## What v1 of the facet does not model

Following [the nginx precedent](#what-this-facet-does-not-model), the gaps are stated rather
than discovered.

- **No message counts, connections, channels or consumers.** Workload rather than host state,
  and not volatile fields to be annotated either: a default view reporting queue depth teaches
  the operator that the tool is noisy. `export_definitions` helps by carrying durable
  entities only.
- **No `environment` beyond named keys**, for the shape reason above.
- **Clustering is this node's view of its cluster**, not a cluster-wide read. rastro
  fingerprints one box.
- **A broker in a container is not this facet's subject.** There is no CLI on the host to
  find, and the `containers` facet already reports the tenant.
- **The collector ships as version `1`**, per
  [the release rule](#every-collector-is-version-1-until-rastro-has-a-release).

## A runtime parameter's value is withheld whole, and no component is trusted by name

Measured, by seeding the entries the first export had none of:

| parameter | what its value carries |
| --- | --- |
| `shovel/my-shovel` | `"src-uri": "amqp://shovel-user:hunter2@upstream.example.com"` |
| `federation-upstream/my-upstream` | `"uri": "amqp://fed-user:s3cret@peer.example.com"` |
| `operator_policy/capped` | `[["max-length", 5000]]`, no credential at all |

So a parameter is credential-bearing by nature rather than by exception, and the credential is
*inside* a URI rather than in a field a reader could name. The whole value is therefore one
`sensitive` text scalar carrying its own JSON spelling, which is the rule
[the container facet applies to an environment variable](#every-environment-value-is-sensitive-and-none-of-them-is-judged-by-name)
and for the reason that entry gives: a plugin may define any component, so an allowlist would
have to be right about software rastro has never seen.

**Withholding the value whole settles two problems beside the credential.** A parameter's
value is arbitrary JSON and arrives in more than one shape: an object for a shovel, and an
Erlang proplist of two-element lists for a global parameter, `[["answer",42],["fraction",0.5],
["on",true]]`. That `0.5` is a floating-point number, which
[the format does not admit](#the-format-admits-no-floating-point-numbers). Carried as text
there is no shape to interpret and no float to render, and the stand-in still changes whenever
the parameter does.

**Cost, and it is a real loss rather than a tidy one.** An operator policy is exported as a
runtime parameter: the document has no `operator_policies` key at all, which was measured
rather than assumed, and `component: "operator_policy"` is where one arrives. Its definition
holds no secret and is now withheld along with everything else, so a diff says an operator
policy changed without saying how. An allowlist of components whose values are structural
would recover it, and it is deliberately not in this change: it needs the same fail-closed
argument the password schemes got, and the safe direction to be wrong in meanwhile is this
one.

## A policy definition keeps its own types, and a number the format cannot carry keeps its spelling

A policy's `definition` is a proper JSON object, unlike the proplist a parameter's value can
be, so its values are read rather than withheld: `"max-length": 1000` stays an integer and
`"queue-mode": "lazy"` stays text. **Typed rather than all-text, because a value that changed
type would otherwise read as unchanged**, which is the same argument
[the redaction digest makes](#redacting-a-sensitive-value) for tagging its own domains.

A non-integer number becomes text carrying the spelling the broker printed. Rounding it would
report a policy the broker does not have, and the format admits no float; the spelling still
changes when the value does, which is what the document is for. A nested list or object does
the same, as its compact JSON spelling: policy definitions are flat in every shape RabbitMQ
documents, so that branch is an honest fallback for a shape nobody has measured rather than a
model of one.

## A boolean could not say "could not tell", and a live broker proved it

The node attribution above shipped as a boolean: either a process that booted RabbitMQ holds
the distribution port or it does not. The first run of the finished facet against a live
broker reported `runs_rabbitmq: false` for a broker that was plainly running.

**The behaviour was right and the report was wrong**, which is the worse of the two failures.
rastro had declined to address the node, which is the safe thing to do with no evidence, and
had then written a confident denial about the box. The cause was measured rather than guessed:
in a container whose capabilities are reduced, `/proc/<pid>/fd` of a process owned by another
account cannot be read even as root, and `CapEff: 800405fb` is what podman gives by default.
Without those descriptors nothing can be joined to the socket the port names, so the holder is
invisible and a boolean has nowhere to put that.

So the answer is three-valued, the way
[`Presence`](../crates/rastro-collector/src/lib.rs) already is, over five named host states:

| evidence | `runs_rabbitmq` | addressed |
| --- | --- | --- |
| a process that booted rabbitmq holds the port | `true` | yes |
| another erlang application holds the port | `false` | no |
| no socket in the table offers the port | `false` | no |
| the holder of the port could not be read | `null` | no |
| no socket table could be read | `null` | no |

The document carries the tri-state and the evidence in words, rendered from **one** field, so
the answer and the reason for it cannot drift apart. The two `null` cases are the ones this
entry exists for, and they are different facts: a descriptor rastro may not read, and a
`/proc/net` that is not there at all.

**Why this is not merely a nicety.** The facet's whole restraint is that it does not address a
node it cannot vouch for. That restraint is invisible in the output unless the output can say
why it held back, and an operator reading `false` would reasonably conclude the broker they
can see running is not a broker. A fingerprint that is wrong about a box in a way the box
cannot correct is worth less than one that admits the gap.

**Where this cannot be tested.** A container cannot exercise the confirmed case at all without
`--cap-add=SYS_PTRACE`, which is why the live-broker workflow passes it and says so in a
comment. The fixtures cover all five states, because a test builds its own `/proc` and can
therefore produce a port whose holder is unreadable without needing a kernel that refuses.

## The store is sealed from a descriptor the broker holds open, not from a second read

The tree to seal is the node's message store, and the claim phase is the awkward place to
learn its path. Claims are gathered before any collector runs and before the walk,
*sequentially*, in the composition root, so anything a claim needs is paid on the critical
path of every run. A `status` read there costs 303, 326, 303, 310 and 308 ms across five
consecutive measurements, each one an Erlang VM boot.

**Three options were weighed and the measurement produced a fourth.**

| | invocations | wall clock | path |
| --- | --- | --- | --- |
| ask twice, claim and collect each reading `status` | 2 | ~310 ms | resolved |
| read once and memoise it for both phases | 1 | ~310 ms | resolved, slightly staler |
| claim nothing | 0 | 0 | nothing sealed |
| **read `/proc`** | **0** | **~0** | **resolved** |

The first two are nearly identical in wall clock, which is not how this started out being
argued. The collect-phase read happens on a pool of four alongside twenty-two other
collectors, so it is hidden; the claim-phase read is not, because that phase is serial. So
memoising saves an invocation and almost no time, and the only real question was whether the
claim phase has to ask the broker at all.

**It does not, because a running broker holds its own store open.** Measured on a live node:
ten descriptors under the store, `cwd` at the mnesia base, and the shallowest descriptor a
quorum queue's write-ahead log at
`/var/lib/rabbitmq/mnesia/rabbit@<node>/quorum/rabbit@<node>/00000001.wal`. rastro already
walks those descriptors to attribute the node, so the path costs one `readlink` it was going
to make anyway.

**The first component named for the node decides the root**, which is not fussiness: the node
name appears twice in that path, and taking the last occurrence would seal a subtree of the
store and leave the rest of it in the walk.

**Resolved rather than assumed, and the usual escape is shut.**
`/var/lib/rabbitmq/mnesia/<node>` is Debian's default and not a rule, since
`RABBITMQ_MNESIA_DIR` moves it, and
[the postgres claim](#a-clusters-registered-data-directory-is-in-the-facet) records what a
claim over an assumed default costs. The environment variables that would say where the store
really is cannot be read off the process either: the broker's beam carries **no environment
variables at all**, measured.

**`cwd` was considered and rejected as the fallback.** It is resolved, it is one `readlink`,
and it is the mnesia *base* rather than the store: sealing it would also cover
`.erlang.cookie`, whose mode the walk reports today and which this facet does not yet describe
itself. So a broker holding nothing under its store makes no claim, on the postgres rule that
a failed read makes none.

**Cost:** a run that cannot read the broker's descriptors seals nothing, which is the same
capability that decides
[whether a node can be attributed at all](#a-boolean-could-not-say-could-not-tell-and-a-live-broker-proved-it),
and it fails in the safe direction: a noisy subtree in the document rather than a tree sealed
on a guess.

## The Erlang runtime can speak before the document does

The first run of the live-broker job failed every read of the facet with
`rabbitmqctl status did not answer with a JSON document`. The tool had written this to stdout
ahead of its document:

```text
=ERROR REPORT==== 23-Sep-2026::14:03:05.184639 ===
file:path_eval(["/var/lib/rabbitmq","/home/runner/.config/erlang"],".erlang"): permission denied
```

A parse that started at the first byte saw `=` where it wanted `{`.

**It could not be reproduced**, and that is what decided the shape of the fix rather than a
taste for leniency. The same version, 3.12.1, on Ubuntu 24.04 in a container answers at byte
0: as root with `HOME` set, with a cleared environment, and with `HOME` pointing at a
directory that does not exist. Whatever the runner does differently, the runtime's report is a
property of the host rather than of the version, so rastro cannot know in advance which boxes
produce it and has to be able to read past it.

**Past it, and no further.** The document begins at the first line that starts with `{`, so a
preamble is skipped whole lines at a time rather than by hunting for a brace, which an Erlang
term inside a report could perfectly well contain. Output with no such line is handed to the
parser unchanged, so the failure still quotes what the tool actually said: a usage dump is
still a failed read rather than an empty document.

**Both JSON reads go through it**, status and definitions, because both come from the same
tool on the same stdout.

**What this also caught: the facet had only ever met one RabbitMQ.** Every measurement behind
these entries was taken on Debian 13 with 4.0.5, and the runner has Ubuntu's 3.12.1. The
document differs in two ways the parse now has a fixture for: 3.12 carries
`release_series_support_status`, which 4.0 does not, and it has no `tags` key at all. Both
were already handled, by ignoring unknown fields and defaulting absent ones, but *handled by
construction* and *shown to work* are different claims and only one of them is worth making.

## A node's name is read from the box, never composed

The facet keyed itself on `local@host`, built from the register's local part and the box's
hostname. That is a guess wearing a reading's clothes, and it has a case where it is simply
wrong: a node started with `RABBITMQ_USE_LONGNAME` calls itself `rabbit@broker.example.test`
while the composition says `rabbit@broker`. rastro would have keyed the facet on a name
nothing answers to and addressed `rabbitmqctl -n` with it.

**The broker writes its own name into the directories it holds open**, measured on RabbitMQ
3.12.1 and 4.0.5, with the default store, with a relocated one, and under long names:

```text
<store>/coordination/<node>/names.dets
<store>/quorum/<node>/00000001.wal
```

`coordination` and `quorum` are Ra's system directories. RabbitMQ puts them directly under the
data directory and each holds one subdirectory named for the node, so the component after the
bucket is the node's own name, whatever it happens to be.

**The facet keys on what the register calls the node**, not on that name, and the two are
different on purpose. epmd names every node on the box and always answers; the node's own name
is read from files an unprivileged run cannot see. Keying on the half that is always there
keeps one key shape, and `node_name` sits inside the entry where it is allowed to be absent
without leaving a node unkeyed.

**A node rastro cannot name is a node rastro does not address.** `-n` takes the name the node
runs under and nothing else, so an unreadable name means the reads are skipped and the entry
says so, rather than a guess being sent to a broker.

## The store is found by Ra's directories, not by the node's name

Superseded rule: [the store was taken as the prefix up to the first component named for the
node](#the-store-is-sealed-from-a-descriptor-the-broker-holds-open-not-from-a-second-read).
That holds only for the default layout. With `RABBITMQ_MNESIA_DIR=/srv/rabbit-data` the store
root carries no node name at all, while `/srv/rabbit-data/quorum/rabbit@host/` still does, so
the old rule sealed the Raft subtree and left the message store in the walk — the exact
failure the seal exists to prevent, and the one the review caught.

The shape above answers this too: **everything before the bucket is the store root**, whatever
it is called. Measured on both versions, both layouts. No bucket open means no claim, which is
the postgres rule and the safe direction.

## The facet runs alone, because it is what the other collectors would notice

Every read of a node boots an Erlang VM that joins the broker's distribution cluster and binds
a port for as long as the call lasts. On the shared pool of four, that ephemeral listener and
its `beam.smp` race the `sockets` and `processes` collectors reading the same box, so which of
them a run records is decided by thread scheduling and two runs of an unchanged host differ.

So the collector declares itself
[`Exclusive`](#collectors-run-concurrently-and-the-walk-runs-alone). The walk is exclusive
because it would notice another collector's temp file; this one is exclusive because it *is*
the thing another collector would notice. The cost is that its second or so no longer overlaps
the pool.

**The live-broker workflow could not have caught this**, and that is worth recording: it
compared only the `rabbitmq` facet between two runs, so a difference rastro caused in
`sockets` was outside what it looked at.

## A running broker is not hidden by a missing client

`presence` was `rabbitmqctl` alone, so a box with a broker up and no client installed reported
no RabbitMQ at all. The inventory's clientless path was written, tested and unreachable: the
framework never calls `collect` on an absent facet, and the test exercised the inventory
directly, so it passed over a path production could not take. **A test that green-lights dead
code is worse than no test**, because it reads as coverage.

Presence is now installed **or** running: the client's presence, or a process on the box that
booted RabbitMQ. Both are readings of the host rather than of rastro's own equipment, which is
what presence is supposed to be about.

## Alarms are recorded and annotated, feature flags are recorded and are not

Two reads the first version skipped, both asked for and both worth their invocations.

**An alarm is volatile, measured by raising one.** `rabbitmqctl set_vm_memory_high_watermark
0.0001` puts `{"type": "resource_limit", "resource": "memory"}` into `status`, and it clears
when the pressure does. So it is annotated `volatile`: out of the diffable view, into
`--include-volatile`. It is recorded rather than dropped because while an alarm is up the node
**blocks publishing connections**, and a box that looks healthy and refuses writes is exactly
what an operator opens a fingerprint to explain. The alarm's own `node` field is dropped,
since it repeats the entry's key.

**A feature flag is the opposite of volatile.** Enabling one is deliberate and
**irreversible**: a node that has enabled `khepri_db` cannot go back, and cannot cluster with
one that has not. The set of enabled flags therefore decides what the box can be upgraded to
and joined with, and it appears in no package version and no configuration file. It costs a
third invocation per node, which is the clearest case in this facet of a read earning its
300 ms.

## Owed: a parameter allowlist the operator owns, not one rastro ships

Left deliberately: every runtime parameter's value is withheld, including an operator policy's
definition, which holds no secret. The obvious fix is an allowlist of components whose values
are structural, and the obvious objection is the one
[`.gitleaks.toml`](../.gitleaks.toml) already makes about allowlists: it is a standing
exemption for a shape, and someone can put a secret inside an `operator_policy` whenever they
like.

So the shape it should take, when it is built, is **an allowlist in the operator's own config
file rather than a list rastro ships**: the operator knows their box, which is the same
argument that lets a config rule beat a collector's claim. It carries one requirement that is
not optional, and it is why this is not a one-line change: wherever a value appears because
sensitivity was overridden, the document has to say so, loudly and at the value, so no reader
of a fingerprint can mistake a disclosed secret for one that was never sensitive.

## A long-name node needs `--longnames`, and a short-name node must not have it

Reading a node's real name made addressing long-name nodes reachable, and incomplete: the
name arrived whole while the invocation did not change. Measured on 4.0.5, with a node started
as `rabbit@broker.example.test`:

| invocation | result |
| --- | --- |
| `-n rabbit@broker.example.test status` | exit 65, `invalid node name` |
| `--longnames -n rabbit@broker.example.test status` | exit 0 |
| `--longnames -n rabbit@shorthost status` | **exit 124**, killed at the bound |
| `-n rabbit@shorthost status` | exit 0 |

So every read of a long-name box would have failed and taken the facet with it, which is the
review's point. The second half is the one the measurement adds: **the flag cannot be passed
defensively**. Against a short-name node it does not fail, it *hangs*, and rastro would have
wedged every ordinary box for the tool's full time bound before reporting an error.

**A dot in the host half decides it**, which is Erlang's own rule rather than a guess: `-sname`
refuses a host containing a dot, so a name that has one came from `-name`. The question is
asked per node, in one place that every read goes through, because a node addressed the wrong
way does not fail politely.

**The environment cannot carry it.** `RABBITMQ_USE_LONGNAME` would do the same job, and
[the execution seam clears the environment](#rastro-does-not-change-the-host-it-describes) on
purpose: an inherited environment is an input nobody audited. Measured both ways, the flag is
the only route that works under a cleared environment.

**Every read goes through one place, and the first attempt at this did not.** The flag reached
`status` and not the other two reads, so a long-name box would have had its first read succeed
and its second fail, erroring the whole facet — caught in review rather than by the test, which
is the part worth recording. The test asserted that `--longnames` appeared *somewhere* in what
a recording shim captured, and the first invocation satisfied it; the shim also exited non-zero,
so the reads that were wrong were never even attempted. It now asserts the count of
invocations and checks each one, and putting the bug back makes it fail naming the offending
call.
