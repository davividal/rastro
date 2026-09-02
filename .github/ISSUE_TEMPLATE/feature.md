---
name: State rastro does not capture
about: A change to a host that would go unnoticed in a before-and-after diff
labels: enhancement
---

**What change would go unnoticed today**

<!-- The premise is a box nobody documented, so the useful framing is a change
     somebody could make to a host that two rastro runs would not show. -->

**How the state is observed on a live host**

<!-- The command or the file, and its output. This is the part that decides
     whether the idea is buildable, so it is worth more than the description. -->

```

```

- Does the tool offer a machine-readable form (`--json`, `-o json`, a database of
  its own), or only text a parser would have to guess at?
- Is this **effective, resolved** state, or a config file stating intent? rastro
  prefers the former: `nginx -T` over the conf, `sysctl -a` over `sysctl.conf`.

**Which layer**

- [ ] Layer 1, the filesystem walk
- [ ] Layer 2, the fixed kernel and OS-runtime list, which must not grow with the
      number of tenants on a box
- [ ] Layer 3, service-internal state, dispatched from something Layer 2 already
      observed rather than guessed at
- [ ] A change to a collector that already exists

**Absence and failure**

- How would rastro tell "this host has none of it" from "rastro could not look"?
  The first is `absent`, the second is `error` with a reason, and the two must not
  be confusable.

**Volatility**

- Does any of this change on its own between two runs of an unchanged host (a pid,
  a counter, a timestamp)? Those values are kept but annotated, and left out of the
  default diffable view.

**Privilege**

- Does reading it need root, or a drop to some service account?

**Anything else**

<!-- Prior art, a distribution where this is spelled differently, or a reason the
     obvious approach is wrong. -->
