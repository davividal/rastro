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
| `mounts`     | state    | yes        |

Metadata collectors cannot be excluded: without them one fingerprint cannot be
told apart from another.

## What exclusion does

The facet is omitted from the document, not recorded `absent`, and a line goes
to stderr. The exclusion itself appears in the `invocation` facet, alongside the
view, because both change what is in the document:

```json
"config": {
  "excluded_collectors": ["mounts"],
  "view": "diffable"
}
```

so two runs under different scope cannot be diffed without the difference
showing. The config file's path is deliberately not recorded: a fingerprint is
made to be shared, and a path carries a username and a deployment layout.

Exclusions are sorted and deduplicated, so two configs meaning the same thing
produce the same document.

There is no include list. An exclusion that is wrong produces noise; an
inclusion that is wrong produces a blind spot.
