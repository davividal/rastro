---
name: Bug report
about: rastro reported something wrong, missed something, or would not run
labels: bug
---

<!-- A fingerprint is sensitive operational data: it inventories every package
     version and names every path on the box. Paste the excerpt of the facet in
     question, never the whole document, and redact what you need to. -->

**Version**

Output of `rastro --version`, and whether it came from the `rolling` pre-release
or a local build:

**What happened**

<!-- Which facet, and what it says that is wrong. -->

**What I expected instead**

<!-- If the host's real state is the point, say how you know it: the command you
     ran by hand, and what it printed. -->

**How it was invoked**

- Exact command line:
- As root, via `sudo`, or unprivileged:
- Through [`rastro-ssh`](https://github.com/davividal/rastro/blob/master/rastro-ssh)? (yes / no)
- Config file used? Paste it if so, it is short by design:

**The host**

- Distribution and version:
- `uname -a`:
- init system, and whether it is systemd:
- Anything unusual about the filesystem layout (network mounts, containers,
  read-only root):

**The facet**

```json

```

**What the underlying tool says**

<!-- Most facets come from a canonical tool rather than from rastro's own parsing.
     If you know which one (systemctl, ip, lsblk, pg_lsclusters, dpkg, apk, ...),
     its raw output for the same thing is the single most useful thing in this
     report. -->

```

```

**stderr**

<!-- Warnings and errors go to stderr, never into the document. `--debug` adds
     per-collector timings if the complaint is about cost. -->

```

```
