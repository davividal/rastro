#!/usr/bin/env bash
# Runs the suite in a Linux container, on Debian and on Alpine, from a working
# machine. CI does the same thing from `.github/workflows/container.yml`; both call
# `container-suite.sh` inside the container, so neither can quietly drift from the
# other.
#
#   scripts/test-in-container.sh                 # both images
#   scripts/test-in-container.sh rust:alpine     # one of them
#
# Why this exists at all: off Linux, seven tests cannot run. Six want /proc and the
# seventh wants a unix socket path shorter than SUN_LEN, so a green macOS run has
# proved the fixtures and not the walk, which is most of the product.
set -euo pipefail

# Debian and Alpine because the package sources differ: dpkg and apk are two separate
# branches of the `packages` collector, and a working machine exercises neither.
#
# Deliberately floating rather than pinned by digest, unlike the actions in the
# workflows. What this asks is whether rastro still works on today's Debian and
# today's Alpine, and a pinned image answers that about the day it was pinned.
if [[ $# -gt 0 ]]; then
    IMAGES=("$@")
else
    IMAGES=(rust:latest rust:alpine)
fi

ENGINE=$(command -v podman || command -v docker) || {
    echo "neither podman nor docker is on PATH" >&2
    exit 1
}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# CARGO_TARGET_DIR is not optional. CARGO_TARGET_TMPDIR is derived from it, and the
# default puts every scratch tree back on the bind mount, which on macOS is not the
# Linux the container was started for: it refuses a filename that is not UTF-8 and
# refuses a unix socket, so the walk tests for both fail there and pass on any real
# Linux. Keeping the build off the mount also means a run leaves no root-owned
# `target/` behind in the checkout.
#
# The named volume is one registry cache across runs and images, so each invocation
# does not download the index again.
for image in "${IMAGES[@]}"; do
    echo "### $image"
    "$ENGINE" run --rm \
        -v "$REPO_ROOT":/w \
        -w /w \
        -e CARGO_TARGET_DIR=/tmp/target \
        -v rastro-cargo-registry:/usr/local/cargo/registry \
        "$image" sh /w/scripts/container-suite.sh
done
