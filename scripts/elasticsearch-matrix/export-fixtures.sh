#!/usr/bin/env bash
# Copies the /proc side of every captured cell into rastro's test fixtures.
# Usage: export-fixtures.sh CAPTURES DEST
#   CAPTURES: the VM's /captures, copied out (`vagrant ssh -c 'sudo tar -C /captures -czf - .'`)
#   DEST:     crates/rastro/tests/fixtures/elasticsearch/cells
set -eu

readonly PROCESS_FILES=(cmdline environ stat status fd.list)
readonly NODE_FILES=(version-jar elasticsearch.yml config-dir.ls)

export_node() {
  local node=$1 out=$2 side file
  for side in server parent; do
    [[ -d "$node/$side" ]] || continue
    mkdir -p "$out/$side"
    for file in "${PROCESS_FILES[@]}"; do
      [[ -f "$node/$side/$file" ]] && cp "$node/$side/$file" "$out/$side/$file"
    done
  done
  mkdir -p "$out/files"
  cp "$node/config-dir" "$out/config-dir"
  for file in "${NODE_FILES[@]}"; do
    [[ -f "$node/files/$file" ]] && cp "$node/files/$file" "$out/files/$file"
  done
  return 0
}

export_cell() {
  local cell=$1 dest=$2 name node
  name=$(basename "$cell")
  mkdir -p "$dest/$name"
  # Always written, so a cell with no node is still a directory git keeps.
  cp "$cell/node-count" "$dest/$name/node-count"
  for node in "$cell"/node-[0-9]*; do
    [[ -d "$node" ]] || continue
    export_node "$node" "$dest/$name/$(basename "$node")"
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

# Cell 16 ran an amd64-only image under qemu-user on the aarch64 VM. qemu's binfmt `P` flag
# prefixes the argv with its interpreter and the executable path, which no amd64 box shows.
python3 - "$dest" <<'PY2'
import pathlib, sys
for cmdline in pathlib.Path(sys.argv[1]).glob('*/node-*/*/cmdline'):
    tokens = cmdline.read_bytes().split(b'\0')
    if tokens[0].startswith(b'/usr/libexec/qemu-binfmt/'):
        cmdline.write_bytes(b'\0'.join(tokens[2:]))
        print(f'stripped the qemu-user prefix from {cmdline}')
PY2

cat > "$dest/README.md" <<'README'
# Captured Elasticsearch cells

The `/proc` side of each cell of `docs/elasticsearch-matrix.md`, captured on real nodes by
`scripts/elasticsearch-matrix/capture.sh` and copied here by `export-fixtures.sh` beside it.
Regenerate rather than edit.

One edit is made on export: cell 16 ran an amd64-only 6.8 image under qemu-user, whose binfmt
`P` flag prefixes the argv with its interpreter and the executable path. Those two tokens are
removed, so the argv is the one an amd64 box shows.
README
du -sh "$dest"
