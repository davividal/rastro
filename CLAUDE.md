# CLAUDE.md

Guidance for Claude Code (claude.ai/code) in this repository.

## What this is

**rastro** — a server state fingerprint generator (AGPL-3.0, OSS). One static
Rust binary, dropped on a Linux box, emits a canonical diffable JSON fingerprint
of the host. Purpose: see what a change *actually did* to an undocumented
server, via before/after fingerprints and a plain diff.

**It runs.** Both views work, config narrows a run. `built_in()` in
`crates/rastro/src/collectors.rs` says which collectors ship.

**Not built:** the Layer 1 walker, the rest of Layer 2, Layer 3, the exec
contract, redaction.

The toolchain is pinned in `mise.toml`, and CI reads the same file.

```sh
cargo test                                    # the whole workspace
cargo clippy --all-targets -- -D warnings     # CI treats warnings as errors
cargo fmt --all                               # CI runs --check
cargo build --release --target x86_64-unknown-linux-musl
```

Off Linux there is no `/proc`, so the three `tests/cli.rs` tests that read the real
host fail while every fixture test passes. Gate those in a container, Alpine as well
as Debian since the package sources differ:

```sh
podman run --rm -v "$PWD":/w -w /w \
  -v rastro-cargo-registry:/usr/local/cargo/registry rust:latest sh -c 'cargo test'
```

The named volume stops each run re-downloading the registry. A container leaves a
root-owned `target/`.

## Documents

- `README.md` — the pitch.
- `docs/design.md` — architecture and the output-format contract. Read before
  changing anything structural.
- `docs/decisions.md` — the decision log. Reversing an entry means a new entry,
  never a silent edit.
- `docs/config.md` — the config file reference.
- `docs/research.md` — prior art. AIDE and configsnap were **rejected as
  dependencies**; collectors are native.
- `.ai-jail` — read-protected; do not read, modify or delete it.

## Design invariants

Violating one is a plan change, not a detail.

- **The output format is the contract.** Fixed key order (declared where rastro
  owns the shape, sorted where a collector does), defined list ordering, no
  floating point, volatile values omitted from the diffable view. Two runs on an
  unchanged box are byte-identical, which is why there is no diff verb.
- **Everything observed comes from a collector**, host identity and invocation
  metadata included. The envelope holds only `schema_version`, `metadata[]` and
  `facets[]`.
- **Collectors classify, renderers present.** Volatility and sensitivity are
  per-value annotations; what to do about them is decided at render time.
- **Three layers:** L1 filesystem walker, L2 fixed OS-runtime list, L3
  service-internal state discovered from L2 signals, never guessed. Prefer
  effective state over config files (`nginx -T`, not nginx conf).
- **Exclusions, never inclusions.** Config is optional and can only narrow.
- **Absence is state.** Statuses are `ok|absent|error`; excluded collectors are
  omitted with a WARN. Failures are loud in the output, never silent.
- **Envelope self-description:** the `invocation` facet carries the binary
  version and the effective config.
- **Secrets** are hashed by default, `--raw` opts out. Redaction is a collector
  responsibility, an option not a guarantee.
- **stdout carries only the fingerprint.**
- v1 boundaries: single box, generate-only, no network I/O, JSON only.

## Comment scope

The core conventions cap an inline `//` comment at one line. Applied here that means **one
non-obvious thought, stated fully**, which is occasionally two lines and is not licence for a
paragraph. Doc comments (`///`, `//!`) are the opposite: they are expected to carry the *why*,
including the mechanism in another crate or in the kernel that a reader cannot infer.

Test `// Arrange:` comments are doc comments in spirit. They state the host or crate behaviour a
fixture stands for, and trimming them to fit a line for production code would delete the reason
the fixture looks the way it does. Two reviewers have raised the literal one-line rule against
this file's own practice, so it is written down rather than re-litigated.

## Guiding principles

PEP 20 and Clean Code, neither to the letter, with Rust's idiom as the lens.
Clean Code wins where they conflict. Three Clean Code positions are wrong here:

- **Extract till you drop.** `facet_of` is one `match` whose value is that the
  absence-and-failure rule is visible in one place.
- **Comments are a failure to express intent.** The comments here carry *why the
  obvious alternative is wrong* — why kernel order in `/proc/mounts` is kept,
  why splitting mount options on every comma corrupts SELinux contexts.
- **Replace switch with polymorphism.** An exhaustive `match` on an enum *is* the
  safety mechanism: add a variant and the compiler names every site.

## Working conventions

- Debian/systemd first; choices must not need breaking changes to generalise.
- **Planned, not built:** a tag-triggered release job with checksums.
- **Separation is enforced by cargo.** `rastro-fingerprint` (the document)
  depends on nothing of ours; `rastro-collector` (the port, and what an outside
  contributor depends on) depends on it; `rastro` (the tool) on both. Neither
  library crate may read the host, and each polices itself in `tests/purity.rs`.
  `main.rs` is the composition root and holds no decision worth testing.
- Adding a collector: one file under `crates/rastro/src/collectors/`, a `mod`
  and `pub use` line, and an entry in `built_in()`.
- The flagship test is the **determinism harness**: two runs, byte-identical.
  Any new collector or format change must keep it green.
- **TDD.** A new feature starts with a test that fails.
- **Commit at every green.** A red-green cycle is one commit.
- **A commit message is a subject line.** Imperative, under 50 characters, no
  full stop. Needing a body means the commit is too big.
- **DDD.** New features carry a domain model, and the code reflects it.
