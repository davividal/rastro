# Contributing

rastro is one static binary that an operator runs as root on a box they inherited
and cannot afford to disturb. Most of what follows exists because of that, rather
than because of taste.

Read [docs/design.md](docs/design.md) before changing anything structural, and
[docs/decisions.md](docs/decisions.md) for what was already chosen and what it
cost. Reversing a decision means a new entry in that log, never a silent edit to
the old one.

## Setting up

The toolchain is pinned in [`mise.toml`](mise.toml), and CI reads the same file,
so a working machine and a CI runner cannot drift.

```sh
mise install
cargo nextest run
```

Without [mise](https://mise.jdx.dev), install the version `mise.toml` names with
rustup, along with the `clippy`, `rustfmt` and `llvm-tools` components and the
`x86_64-unknown-linux-musl` target. There is no MSRV floor and no support promise
for an older compiler:
[why](docs/decisions.md#no-msrv-floor-the-toolchain-is-pinned-by-mise).

`cargo test` works. `cargo nextest run` is what CI runs and what
[`.config/nextest.toml`](.config/nextest.toml) is tuned for: it gives each test
its own process, which is what keeps the tests that invoke the real binary from
interfering with each other through the filesystem.

## The gates

Every one of these runs in CI. Run them before opening a pull request.

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo nextest run --workspace
cargo doc --locked --no-deps
cargo deny --all-features --locked check
cargo build --locked --release --target x86_64-unknown-linux-musl -p rastro
scripts/test-in-container.sh
```

Note the argument order on `cargo deny`: `--all-features` and `--locked` are
root-level arguments, and `cargo deny check --all-features` fails with a tip rather
than a scan. `--locked` matters most on this one of all the gates, since it is the
job auditing the dependency graph and so it must audit the graph the others build.

`cargo doc` is a gate rather than a courtesy: intra-doc links are only checked
when the docs are built, which is what makes `deny(rustdoc::broken_intra_doc_links)`
bite. `cargo deny` needs `cargo install cargo-deny`; its policy lives in
[`deny.toml`](deny.toml), and a new licence in the dependency tree is meant to
fail until somebody decides about it.

### Off Linux, seven tests fail

Six of them want `/proc`: five in `crates/rastro/tests/cli.rs` and
`a_config_can_seal_a_tree_so_the_walk_stops_there`, which reads `/proc/mounts`
through a real walk. The seventh, `walk_records_a_socket_and_a_fifo`, wants a unix
socket path shorter than `SUN_LEN`. Every fixture test passes anywhere.

This is expected on a macOS working machine and is not a licence to skip the
container run below.

### The container run is the real gate

A macOS pass proves the fixtures. It does not prove the walk, and the walk is most
of the product.

```sh
scripts/test-in-container.sh              # Debian and Alpine, root and unprivileged
scripts/test-in-container.sh rust:alpine  # one of them
```

Podman or docker, whichever is on `PATH`. Both images, because the package sources
differ: dpkg and apk are two separate branches of the `packages` collector, and
glibc and musl are two different answers about what a filename is.

Both privilege levels, because rastro is run as root on a production box and CI is
not root, and three separate defects have hidden in exactly that difference: a test
that skipped itself as root, a mode assertion that depended on the caller's umask,
and an unreadable mount point that failed a whole facet. The script runs the suite
as root, then creates a user with no privilege and runs it again as them.

`.github/workflows/container.yml` calls the same
[`scripts/container-suite.sh`](scripts/container-suite.sh) inside the container, so
what CI asks and what you just asked cannot drift. It runs on every push to master,
nightly, on demand, and on a pull request labelled `container`. **Add that label**
to a pull request touching the walk, the `packages` collector, anything reading
`/proc`, the output file's mode, or the execution seam.

## Working conventions

- **TDD.** A new feature starts with a test that fails. A bug starts with the test
  that reproduces it.
- **Commit at every green.** A red-green cycle is one commit.
- **A commit message is a subject line.** Imperative, under 50 characters, no full
  stop. Needing a body means the commit is too big.
- **Comments carry the non-obvious *why*.** Never a restatement of the code, the
  filename, or git history. See the comment-scope note in
  [CLAUDE.md](CLAUDE.md#comment-scope).
- **Remove what your change orphaned** in the same commit.

## The invariants a change must not break

These are in full in [CLAUDE.md](CLAUDE.md#design-invariants). Breaking one is a
plan change, not a detail. The four that a pull request gets wrong most easily:

1. **The output format is the contract.** Fixed key order, defined list ordering,
   no floating point, volatile values omitted from the diffable view. Two runs on
   an unchanged box are byte-identical, which is why there is no diff verb.
2. **Absence is state.** A collector that found nothing is `absent`. A collector
   that could not tell is `error`, with the reason. Neither is ever a silent
   omission.
3. **The filesystem collector reads metadata and opens no file.** An entry is one
   digest of exactly the attributes the view keeps.
4. **Exclusions, never inclusions.** Config is optional and can only narrow a run.

The determinism harness is the flagship test, and it is in two halves:
`crates/rastro/tests/cli.rs` compares the envelope and every non-filesystem facet
through the real binary, and `crates/rastro/tests/determinism.rs` compares the
filesystem facet over a tree it owns. Any new collector or format change must keep
both green.

## Adding a collector

One file under `crates/rastro/src/collectors/`, a `mod` and `pub use` line, and an
entry in `built_in()`.

A built-in collector is layered, and the arrows point one way: `source/` holds one
host interface's spelling, `model/` the types that render as a composed node,
`value_objects/` the types that render as a leaf. Nothing in the last two knows a
host interface exists. `crates/rastro/tests/purity.rs` enforces it.

Prefer effective, resolved state over reading a config file, and prefer the source
that is unambiguous over the one that is convenient: ask the tool for JSON where it
offers one, read the manager's own database where it does not. Shelling out goes
through `collectors::canonical_tool` and nowhere else.

A new built-in collector is a scope decision. Open an issue before writing it.

## Documentation

A user-visible change updates the README or the relevant `docs/` page in the same
commit. A decision, or the reversal of one, gets an entry in
[docs/decisions.md](docs/decisions.md).

## Sign-off, not a CLA

rastro is AGPL-3.0-only, and contributions come in under a
[Developer Certificate of Origin](https://developercertificate.org) sign-off
rather than a CLA. That suits a solo maintainer and keeps the barrier to a first
patch low:
[why](docs/decisions.md#licence-agpl-30-only).

Sign off every commit. The trailer is a statement that you wrote the patch, or
otherwise have the right to submit it under the project's licence.

```sh
git commit -s
git commit --amend -s          # the last commit
git rebase --signoff origin/master   # a whole branch
```

The name and email in the trailer must be your own and must match the commit
author. A sign-off in somebody else's name is worth nothing.

## Check the identity on your commits before pushing

GitHub attributes a commit through the author email stored in it, and a checkout
that inherited a global identity from an employer's machine is the usual way a
patch lands under the wrong name.

```sh
git log --format='%h %an <%ae>' origin/master..HEAD
git log --format='%h %(trailers:key=Signed-off-by,valueonly)' origin/master..HEAD
```

Set the right identity for this checkout when the global one belongs elsewhere:

```sh
git config --local user.name "Your Name"
git config --local user.email "your-verified-address@example.com"
```

Fix attribution on the branch, before the merge. Published history is not
rewritten to correct it afterwards, because that invalidates every commit hash and
breaks existing clones.

## Security

Do not open a public issue for a vulnerability. [SECURITY.md](SECURITY.md) has the
reporting channel and the threat model, including what rastro deliberately does
not defend against yet.
