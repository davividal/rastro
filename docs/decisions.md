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
