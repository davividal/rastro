# Captured Elasticsearch cells

The `/proc` side of each cell of `docs/elasticsearch-matrix.md`, captured on real nodes by
`scripts/elasticsearch-matrix/capture.sh` and copied here by `export-fixtures.sh` beside it.
Regenerate rather than edit.

One edit is made on export: cell 16 ran an amd64-only 6.8 image under qemu-user, whose binfmt
`P` flag prefixes the argv with its interpreter and the executable path. Those two tokens are
removed, so the argv is the one an amd64 box shows.
