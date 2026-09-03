#!/bin/sh
# Runs the test suite on a real Linux, twice: as root, then as a user holding no
# privilege at all. Meant to be executed *inside* a container, by
# `test-in-container.sh` on a working machine and by `.github/workflows/distributions.yml`
# in CI, so both ask the same question of the same code.
#
# Why twice. rastro is run as root on a production box, and most of the suite has only
# ever been watched under one of the two. Three separate defects hid in that gap: a
# test that skipped itself as root, a mode assertion that depended on the caller's
# umask, and an unreadable mount point that failed a whole facet. A container job is
# root by default, so the unprivileged half has to be asked for explicitly or it
# silently stops happening.
#
# `cargo test` rather than `cargo nextest run`: nextest is a speed optimisation and
# would have to be compiled into every image first, and the question here is what the
# code does on Linux, not how fast the suite is. The pinned toolchain is not this
# job's question either, which is why the images bring their own compiler.
set -eu

: "${CARGO_TARGET_DIR:=/tmp/target}"
export CARGO_TARGET_DIR
# The rust images put it here. Named explicitly because the unprivileged half has to
# be able to reach it, so it cannot stay an implicit default.
: "${CARGO_HOME:=/usr/local/cargo}"
export CARGO_HOME

UNPRIVILEGED=runnerish
WORKSPACE=$(pwd)

distribution=$( . /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}" )
echo "==> $distribution, $(uname -srm), $(cargo --version)"

echo "==> suite as root"
cargo test --workspace --locked

# busybox has `adduser` and not `useradd`, which is the whole of the difference
# between Alpine and Debian here.
if ! id "$UNPRIVILEGED" >/dev/null 2>&1; then
    if command -v useradd >/dev/null 2>&1; then
        useradd -m "$UNPRIVILEGED"
    else
        adduser -D "$UNPRIVILEGED"
    fi
fi

# A directory of their own for the tests that build a tree and walk it. Without this
# they inherit /tmp, where the root run has already left files they cannot replace.
UNPRIVILEGED_TMP="/home/$UNPRIVILEGED/tmp"
mkdir -p "$UNPRIVILEGED_TMP"
chown "$UNPRIVILEGED" "$UNPRIVILEGED_TMP"

# The root run owns the build directory and the registry, and the second run has to
# write into both: cargo takes a lock file in CARGO_HOME even when it fetches nothing,
# and the test binaries land under CARGO_TARGET_DIR. `X` sets the execute bit on
# directories only, so this does not make every artefact executable.
chmod -R a+rwX "$CARGO_TARGET_DIR" "$CARGO_HOME"

echo "==> suite as $UNPRIVILEGED"
# `su` resets the environment, so everything the run needs is restated on the command
# line rather than exported above.
su "$UNPRIVILEGED" -c "cd '$WORKSPACE' \
    && PATH='$PATH' \
    CARGO_HOME='$CARGO_HOME' \
    CARGO_TARGET_DIR='$CARGO_TARGET_DIR' \
    TMPDIR='$UNPRIVILEGED_TMP' \
    cargo test --workspace --locked"

echo "==> $distribution: both runs green"
