# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**rastro** — a server state fingerprint generator, planned as an OSS project
(AGPL-3.0). A single static Rust binary (musl) that is dropped onto a Linux box,
run as root, and emits a canonical, diffable JSON fingerprint of the host's
state to stdout/file. Purpose: see what a change (e.g. a new Ansible role)
*actually did* to an undocumented production server, via before/after
fingerprints and a plain diff.

**Current status: it runs.** `./rastro` emits a real fingerprint. Three
collectors ship: `host` and `invocation` (metadata) and `mounts` (state). Both
views work, config narrows a run, and `crates/rastro/tests/cli.rs` enforces the
determinism contract end to end through the built binary.

Not built yet: the Layer 1 walker, the remaining Layer 2 collectors, Layer 3,
the exec contract, and redaction of `sensitive` values.

```sh
cargo test                                    # the whole workspace
cargo clippy --all-targets -- -D warnings     # CI treats warnings as errors
cargo fmt --all                               # CI runs --check
cargo build --release --target x86_64-unknown-linux-musl
```

No Rust toolchain is installed on this machine. Verify builds in a container:
`docker run --rm -v "$PWD":/w -w /w rust:alpine sh -c 'cargo test'` (it is
musl-native, so it also exercises the shipping target). It writes a root-owned
`target/`; remove it from inside the container afterwards.

## Documents

- `README.md` — the pitch: what rastro is, what it deliberately is not.
- `docs/design.md` — architecture, collector contract, the output-format
  contract, security posture, verification strategy. Read it before proposing or
  changing anything structural.
- `docs/decisions.md` — the decision log. Entries were settled through an
  explicit interview with the maintainer; they are decisions, not suggestions.
  Reversing one is a new entry with its reasoning, never a silent edit.
- `docs/research.md` — the research that preceded the project (three-layer state
  model, AIDE/configsnap evaluation, measured costs). Provenance only: AIDE and
  configsnap were **rejected as dependencies**; rastro's collectors are native.
- `.ai-jail` — intentionally read-protected; do not attempt to read, modify, or
  delete it.

## Design invariants (violating these is a plan change, not a detail)

- **The output format is the real contract.** Canonical serialisation: a fixed
  key order (declared where rastro owns the shape, sorted where a collector
  does), defined list ordering, no floating point, volatile values omitted from
  the diffable view. Two runs on an unchanged box must produce a
  byte-identical diffable view — this is what makes external
  `diff`/`dyff`/`jd` sufficient; there is deliberately no diff verb in v1.
- **Everything observed comes from a collector**, including host identity and
  invocation metadata (category `metadata`, cannot be disabled). The envelope
  holds only `schema_version`, `metadata[]` and `facets[]`.
- **Collectors classify, renderers present.** Volatility and sensitivity are
  per-value annotations set by the collector; what to do about them is decided
  at render time.
- **Three-layer collector model:** L1 filesystem walker, L2 fixed OS-runtime
  list, L3 service-internal state discovered from L2 signals — never guessed.
  Prefer *effective/resolved* state over reading config files (`nginx -T`, not
  nginx conf files).
- **Exclusions, never inclusions**, are the user-tunable scope surface.
- **Absence is state:** an enabled collector with no tenant records `absent`;
  facet statuses are `ok|absent|error`. Disabled collectors are omitted with a
  WARN on stderr. Failures are loud in the output, never silent.
- **Envelope self-description:** invocation metadata carries the rastro binary
  version and the full *effective* config (defaults + file + flags resolved).
- **Config is optional, opt-in and exclusion-only.** With no `--config` every
  collector runs, because the premise is a box nobody documented. A config can
  only narrow; there is no way to name the collectors that *do* run. No
  auto-discovery. An unknown collector name, an attempt to exclude a metadata
  collector, an unknown key, or an unreadable `--config` path all fail the run
  rather than being ignored.
- **Secrets:** sensitive values hashed by default, `--raw` opts out. Redaction
  is a per-collector responsibility, documented as an option, not a guarantee.
- **stdout carries only the fingerprint**; diagnostics go to stderr/log.
- v1 boundaries: single box, generate-only, no network I/O (a v1 limitation,
  not project policy), JSON only (rendering is a decorator seam).

## Guiding principles

Three rulebooks apply on top of the global conventions, and none is followed to
the letter.

**PEP 20, the Zen of Python.** Language-agnostic, short enough to actually check
against, and already load-bearing here: *errors should never pass silently* is
why a failed collector becomes a loud `error` facet rather than a log line;
*in the face of ambiguity, refuse the temptation to guess* is why `Presence` is
three-valued; *explicit is better than implicit* is why config is mandatory and
why the flag is `--include-volatile` rather than `--complete`; *namespaces are
one honking great idea* is the workspace carve under `crates/`.

PEP 20 cannot be followed to the letter by construction: it holds *special cases
aren't special enough to break the rules* and *although practicality beats
purity* three lines apart. It is a set of tensions to weigh, not a checklist,
and that is the point of it.

**Clean Code**, for naming, method decomposition, single level of abstraction,
and treating tests as first-class. Three of its positions are wrong for this
codebase and are not to be applied:

- **Extract till you drop.** `facet_of` is one `match` whose entire value is
  that the absence-and-failure rule is visible in one place; spreading it over
  four functions would hide the rule to satisfy a line count. The Layer 1 walker
  will also run over tens of thousands of entries.
- **Comments are a failure to express intent in code.** The comments here carry
  *why the obvious alternative is wrong*: why kernel order in `/proc/mounts` is
  kept rather than sorted, why splitting mount options on every comma corrupts
  SELinux contexts. No identifier can carry that, and deleting it would cost the
  next reader the bug.
- **Replace switch with polymorphism.** In Rust an exhaustive `match` on an enum
  *is* the safety mechanism: add a `FacetOutcome` or `Presence` variant and the
  compiler names every site that must handle it. Trait objects would delete that
  check in exchange for indirection.

Where they conflict, Clean Code wins over PEP 20. Rust's own idiom is the third
rulebook but not a third precedence: it is the lens the other two are read
through. Apply Clean Code as Rust would, not as Java would, which is what the
three exclusions above are.

## Working conventions for this repo

- Target platform is Debian/systemd first; design choices must not need
  breaking changes to generalise later.
- Repo shape: a Cargo workspace under `crates/`. CI via GitHub Actions runs
  test, clippy, fmt, doc, an MSRV check and a musl cross-compile that uploads
  the binary as an artefact. **Planned, not built:** a tag-triggered release job
  with checksums. DCO sign-off, no CLA.
- **Separation is enforced by cargo, not by convention.** Three crates, and the
  dependency arrows only point one way, because a crate cycle does not compile:
  - `rastro-fingerprint` — what a fingerprint is and its canonical JSON. Depends
    on nothing of ours. Includes the identity types a facet records.
  - `rastro-collector` — the contract a collector fulfils, and how a set of them
    becomes a fingerprint. **This is the crate an outside contributor depends
    on.** Re-exports what an author needs so one dependency is enough.
  - `rastro` — the tool: built-in collectors, the CLI, the wiring.

  `main.rs` is the composition root and delegates: it is the only place that
  knows all three crates, and it holds no decision worth testing.

  Neither library crate may read the host: that is what keeps the model
  buildable on a machine with no `/proc`, which is the machine it is developed
  on, and each polices itself in `tests/purity.rs`. Adding a collector means one
  file under `crates/rastro/src/collectors/`, a `mod` and `pub use` line, and an
  entry in `built_in()`. No repositories: rastro persists nothing.
- The flagship test is the **determinism harness**: full run twice in an idle
  container → byte-identical diffable sections. Any new collector or format
  change must keep it green.
- TDD: Test-driven development is the default workflow. New features must be
  accompanied by a test that fails before the feature is implemented, and passes after.
- **Commit at every green.** A red-green cycle is one commit. The boundary is
  already there, because work starts with a failing test; taking it keeps each
  commit to one behaviour and stops boundaries being found afterwards by
  archaeology, which is how a change ends up too big to describe.
- **A commit message is a subject line.** Imperative, under 50 characters, no
  full stop. Needing a body to explain the commit means the commit is too big:
  split it rather than write the essay.
- DDD: Domain-driven design is the default workflow. New features must be
  accompanied by a domain model that captures the relevant concepts and
  relationships, and the code must reflect that model.
