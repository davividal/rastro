## What changed

<!-- The observable behaviour before and after. If a facet's shape changed, show
     the diff of the rendered JSON rather than describing it. -->

## Why

<!-- Bug fix, new state surface, correctness, cost. Link the issue if there is one. -->

## Gates

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo nextest run --workspace`
- [ ] `cargo doc --locked --no-deps`
- [ ] `cargo deny --all-features --locked check`
- [ ] Ran in a Linux container, Debian **and** Alpine
- [ ] Ran unprivileged, if this touches the walk or the output

<!-- A macOS pass proves the fixtures and not the walk: seven tests cannot run
     there. CONTRIBUTING.md has the podman recipe and the reason
     CARGO_TARGET_DIR is not optional. -->

## Invariants

- [ ] The determinism harness is green in both halves (`tests/cli.rs` and
      `tests/determinism.rs`), and any new value that moves on its own between two
      runs of an unchanged host is annotated volatile
- [ ] Nothing found is `absent`, could-not-tell is `error` with the reason, and
      neither is a silent omission
- [ ] Layer 1 still opens no file
- [ ] Any config addition narrows a run and cannot widen one

## Documentation

- [ ] A user-visible change updates the README or the `docs/` page it belongs to,
      in this same change
- [ ] A decision, or the reversal of one, is a new entry in `docs/decisions.md`
      rather than an edit to the old entry

## Sign-off

- [ ] Every commit carries a `Signed-off-by` trailer in my own name
      ([DCO](https://developercertificate.org), see
      [CONTRIBUTING.md](https://github.com/davividal/rastro/blob/master/CONTRIBUTING.md#sign-off-not-a-cla))
- [ ] I checked the author name and email on every commit in this branch

## Notes for review

<!-- Anything tricky, a trade you made and would defend, or a place you want
     picked at. A cost you accepted knowingly belongs in docs/decisions.md, not
     only here. -->
