# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for a security vulnerability.

Report it as a
[private security advisory](https://github.com/davividal/rastro/security/advisories/new).

rastro has a single maintainer, so the honest commitment is a narrow one:
acknowledgement within 7 days, and no fixed deadline for a fix. You will be
credited in the release notes unless you would rather not be.

## Supported versions

There is no release yet. The only build this policy covers is the current
`master` commit, which is what the `rolling` pre-release republishes. Once 0.1.0
ships, only the latest release will receive fixes.

## Handle a fingerprint as sensitive operational data

This is the most important thing on the page, and it is a property of the output
rather than a bug in it.

A fingerprint is not merely a description of a host, it is a target-selection aid.
The `packages` facet emits a complete name-and-exact-version inventory, which turns
CVE lookup into a filter. `modules` names every loaded driver, out-of-tree and
unsigned ones included. The filesystem facet names every path on the box.

So: do not commit a fingerprint to a repository that is more widely readable than
the box it describes, and do not paste a whole document into an issue. An excerpt
of the facet in question is what a bug report needs.

The output file is created `0600` for this reason. That protects it where it
lands, and nothing protects it after you move it.

A checkout of rastro is the easiest place to break this by accident, since a bare
run drops `rastro-<host>-<UTC>.json` wherever it was started. `.gitignore` covers
that name and `reports/`.

## Threat model

rastro is a **single-box, single-operator, generate-only** tool. It has no daemon,
no server, no socket, no fleet, and no notion of a second user. One box, one
invocation, one document.

### What is defended today

- **The document at rest where rastro writes it.** Created `0600` at creation rather
  than by a later `chmod`, so there is no window in which a file naming every path on
  the box is world-readable. It is rendered into a sibling of the target, flushed and
  `sync_all`ed, and only then published, so a run that died half way leaves no half
  document to be diffed and no partially written file claiming to be one.

  Publishing is `hard_link` without `--force` and `rename` with it. The link fails
  with `EEXIST`, which is what stops a file that appeared while the document was
  rendering from being silently replaced: the one irreversible thing rastro can do is
  destroy the `before` a diff is measured against. The `0600` survives either path,
  where truncating an existing `0644` file would not have, because a mode applies
  when a file is made and not when it is written.

- **Not amplifying privilege.** rastro needs root to read `/etc`, user crontabs and
  the firewall ruleset. It never *gains* privilege. The Layer 3 collectors that must
  speak to a service as its owner drop to that account, whose name is read from
  `pg_lsclusters` rather than assumed to be `postgres`. Dropping and never gaining is
  why this needs no sudoers entry.

- **A collector cannot hang or flood the box it is inspecting.** Shelling out is
  confined to one seam, `collectors::canonical_tool`: absolute path, no shell,
  cleared environment, bounded in time and in output, and a breach kills the tool's
  whole process group rather than only the direct child.

- **No network I/O.** The binary opens no socket. Remote runs are a shell wrapper
  around `ssh` that pushes the binary, runs it and deletes it; they are not a client
  talking to a service. See [`rastro-ssh`](rastro-ssh).

- **No password hashes, structurally.** `/etc/shadow` is read and the hash column is
  dropped where the line is parsed. What reaches the document is a state, the
  placeholder a tool wrote when there is no hash, and the crypt algorithm identifier
  when there is one. No type in the collector has a field a hash could be stored in,
  which is a guarantee an unbuilt redaction layer cannot weaken.

- **No password verifiers, and no digest of an unsalted one.** A PostgreSQL role's
  `rolpassword` is hashed by the server and never read into the process; what the document
  carries is a digest of that hash, which cannot authenticate and, for a SCRAM verifier,
  cannot be matched against a guessed password because the random salt is not kept. Only
  SCRAM is digested. An md5 verifier is `md5(password || rolname)` with no random salt, so a
  digest of it plus the role name beside it in the same document would be an offline password
  oracle; those roles carry no digest at all.

- **Memory safety.** `unsafe` is forbidden by lint across the workspace.

- **The integrity of the published binary.** GitHub Actions are pinned by commit
  rather than by tag, the job holding a token that can write to the repository is
  not the job that compiles, and every published binary carries a build-provenance
  attestation signed against a short-lived OIDC identity. Verify it with
  `gh attestation verify --repo davividal/rastro`; the SHA-256 companion only tells
  you the download did not corrupt. `cargo-deny` gates advisories, licences and
  dependency sources on every change.

- **Not disturbing the host under inspection.** The filesystem collector's walk is the
  part of a run that touches every path on the box, and it stats each one and reads no file's
  contents, so it moves no file's access time and pulls no file data into the page
  cache. It therefore cannot evict the working set of a database it is fingerprinting,
  which is a harm that would land after rastro exited with nothing connecting the two.
  This is a correctness property of a tool you are invited to run on production, and
  it is treated as one.

  It is a property of the walk and not of the whole run. Other collectors do open
  files, a small fixed set of them named in their own source, with the atime and
  page-cache consequences any read has. The distinction that matters here is scale:
  a few kilobytes at known paths, rather than every path on the box.

### Out of scope, today

Named rather than implied, because several of these read as promises elsewhere in
the documentation and are not built yet.

- **`--raw` is not implemented, and redaction now is.** A value a collector marked
  `sensitive` is replaced on the way out by `redacted:sha256+xxh3:` and sixteen hex
  characters: sha256 of the value as lowercase hex, then XXH3-64 over those hex
  characters. It happens in both views, because the complete view is a fuller
  document and not a way round an annotation. The digest proves the value changed
  between two runs without carrying it.

  **It is not protection for a guessable value.** Inverting the pair costs one
  sha256 per guess, so a weak password, a small integer or a boolean is recoverable
  by brute force. Redaction keeps a secret out of the document; it is not a reason
  to put one in.

  **It is an option, not a guarantee**, because marking a field `sensitive` is the
  collector author's job. An unmarked credential is emitted like any other value.

  The design describes `--raw` opting out of redaction with a warning on stderr.
  That flag does not exist yet, so there is currently no way to read a sensitive
  value out of a document, including for an operator who legitimately wants one.

  Two of the three collectors that meet a credential keep it out *structurally*
  rather than by annotation, which is a stronger guarantee than the layer above
  and is unchanged by it: `/etc/shadow`'s hash column is dropped where the line is
  parsed, and a credential-bearing PostgreSQL setting reports `[redacted]` or the
  empty string, so what is recorded is whether it is set.

  **The exposure this section used to name is closed.** `sysctl` annotates
  `net.ipv4.tcp_fastopen_key` and every interface's
  `net.ipv6.conf.<interface>.stable_secret`, and those two are digested rather than
  emitted in cleartext. Excluding the whole `sysctl` collector is no longer the
  answer to them, and it never cost less than the entire facet.

  The previous section still stands whatever is redacted: the document is sensitive
  operational data in full, because it is a fingerprint of every path on the box.

- **A facet's error text is not classified.** A failing collector's message,
  including a bounded tail of a tool's stderr, reaches the document verbatim without
  passing the classification every observed value goes through. Recorded as a known
  exception in
  [docs/decisions.md](docs/decisions.md#a-facets-error-text-is-not-classified-yet).

- **Confidentiality of the document once it leaves rastro.** No encryption at rest,
  no transport of its own. `rastro-ssh` inherits whatever your `~/.ssh/config`
  resolves, host-key checking included, and adds no key management of its own.

- **A hostile host.** rastro *describes* a host, it does not attest one. Anybody with
  root on the box can make the tools it reads say whatever they like, and the
  fingerprint will faithfully record the lie. This is not tamper evidence and not an
  intrusion-detection system; AIDE is the tool for that question, and
  [docs/research.md](docs/research.md) covers why rastro is not trying to be it.

- **Running unprivileged.** Not a security boundary. Without root the collectors that
  read `/etc/shadow`, user crontabs and the firewall ruleset report `error` rather
  than lying about the host, and the run continues. Degrading gracefully without root
  is roadmap, not a sandbox.

- **Third-party collectors.** The exec contract is not built. When it is, an exec
  collector will run with rastro's privilege, which is root. Trusting one will be the
  same decision as trusting any other program you run as root.

- **Denial of service against rastro itself.** A run is bounded per tool, not
  globally, and a host can always be large enough or slow enough that a run does not
  finish. The estimate rastro prints before it starts is a warning, never a limit.

- **Multi-tenancy.** There is none to breach. One operator, one box, one document.
