# Configuration

Optional. With no config, every collector runs. A config can only narrow that.

```sh
rastro --config /etc/rastro.toml
```

The path is always given; there is no auto-discovery.

## Format

```toml
[collectors]
exclude = ["mounts"]
```

| collector    | category | excludable |
| ------------ | -------- | ---------- |
| `host`       | metadata | no         |
| `invocation` | metadata | no         |
| `modules`    | state    | yes        |
| `mounts`     | state    | yes        |
| `packages`   | state    | yes        |

Metadata collectors cannot be excluded: without them one fingerprint cannot be
told apart from another.

## What exclusion does

The facet is omitted from the document, not recorded `absent`, and a line goes
to stderr. The exclusion itself appears in the `invocation` facet, alongside the
view, because both change what is in the document:

```json
"config": {
  "excluded_collectors": ["mounts"],
  "source": "/etc/rastro.toml",
  "view": "diffable"
}
```

so two runs under different scope cannot be diffed without the difference
showing. `source` is `null` when no `--config` was given, and a path that is not
valid UTF-8 is refused rather than recorded lossily.

Exclusions are sorted and deduplicated, so two configs meaning the same thing
produce the same document.

There is no include list. What decides it is which way each kind of list fails
when you forget an entry, and forgetting is the likely mistake on a box nobody
documented.

Forget to exclude a collector and the fingerprint carries a surface you did not
want: noise, visible in the diff, and you exclude it on the next run. Forget to
include one and a whole state surface is missing, with nothing in the output
saying so. Even an exclusion made by mistake stays discoverable, because
`excluded_collectors` is in the `invocation` facet, so a diff shows the scope
changed. A missing inclusion leaves no trace at all.
