#!/usr/bin/env bash
# Copies what every captured cell saw into rastro's test fixtures.
# Usage: export-fixtures.sh CAPTURES DEST
#   CAPTURES: the VM's /captures/redis, copied out (`vagrant ssh -c 'sudo tar -C /captures/redis -czf - .'`)
#   DEST:     crates/rastro/tests/fixtures/redis/cells
set -eu

readonly PROCESS_FILES=(cmdline comm stat status cgroup mountinfo net/tcp net/tcp6 net/unix fd.list unprivileged.txt
  cwd.link root.link exe.link ns/net.link ns/mnt.link ns/pid.link)
readonly SERVER_FILES=(unit unit.show reach server replies.error replies.unauthenticated.error)
readonly CELL_FILES=(server-count facet-root.json facet-unprivileged.json stderr-root.txt stderr-unprivileged.txt
  exit-root exit-unprivileged host-netns.link host-mntns.link host-mountinfo SETUP_FAILED)

export_server() {
  local server=$1 out=$2 file
  mkdir -p "$out/process"
  for file in "${PROCESS_FILES[@]}"; do
    if [[ -f "$server/process/$file" ]]; then
      mkdir -p "$out/process/$(dirname "$file")"
      cp "$server/process/$file" "$out/process/$file"
    fi
  done
  for file in "${SERVER_FILES[@]}"; do
    [[ -f "$server/$file" ]] && cp "$server/$file" "$out/$file"
  done
  for file in files replies cost; do
    [[ -d "$server/$file" ]] && cp -R "$server/$file" "$out/$file"
  done
  return 0
}

export_cell() {
  local cell=$1 dest=$2 name server file
  name=$(basename "$cell")
  mkdir -p "$dest/$name"
  for file in "${CELL_FILES[@]}"; do
    [[ -f "$cell/$file" ]] && cp "$cell/$file" "$dest/$name/$file"
  done
  [[ -d "$cell/host-net" ]] && cp -R "$cell/host-net" "$dest/$name/host-net"
  for server in "$cell"/server-[0-9]*; do
    [[ -d "$server" ]] || continue
    export_server "$server" "$dest/$name/$(basename "$server")"
  done
  return 0
}

src=$(cd "$1" && pwd)
dest=$2
rm -rf "$dest"
mkdir -p "$dest"
for cell in "$src"/[0-9][0-9]; do
  export_cell "$cell" "$dest"
done

# The packaged redis.conf is 146 KB, almost all comment. The server skips a line that is blank or
# starts with `#` after leading blanks, and so does rastro, so those lines are dropped on export.
find "$dest" -path '*/files/*' -name '*.conf' -print0 | while IFS= read -r -d '' file; do
  grep -vE '^[[:space:]]*(#|$)' "$file" > "$file.trimmed" || true
  mv "$file.trimmed" "$file"
done

cat > "$dest/README.md" <<'README'
# Captured redis and valkey cells

What each cell of `docs/redis-matrix.md` showed, captured on real servers by
`scripts/redis-matrix/capture.sh` and copied here by `export-fixtures.sh` beside it. Regenerate
rather than edit.

Per server, `server-N/`:

- `process/`: the server's `/proc` files and the targets of its links, `fd.list` its descriptors,
  `unprivileged.txt` which of them another account could read;
- `unit`, `unit.show`: the unit its cgroup names, and what `systemctl show` said of it;
- `files/`: the files the cell configured, read inside the server's own root at the same paths;
- `replies/`: each reply to rastro's fixed list of commands, the bytes the server sent, without
  credentials (`unauthenticated/`) and after `AUTH` where the cell has a password
  (`authenticated/`);
- `cost/`: `ACL LOG` before and after rastro's runs, and the server's log lines written during them.

Per cell: rastro's facet as root and unprivileged, its stderr and its exit status.

One edit is made on export: blank and comment lines are dropped from every `.conf`, which the
server and rastro both skip. Every password is invented for the matrix.
README
du -sh "$dest"
