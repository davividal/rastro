# Captured redis and valkey cells

What each cell of `docs/redis-matrix.md` showed, captured on real servers by the matrix tooling in
rastro-research (`collectors/redis-expert/matrix/`). Regenerate rather than edit.

Per cell: the host's own TCP socket tables (`host-net/`), mount table and namespace links, which
is what rastro compares each server's against. Per server, `server-N/`:

- `process/`: the server's `cmdline`, `comm`, `stat`, `cgroup`, `mountinfo`, its own TCP socket
  tables, its descriptors as `fd.list`, and the targets of its `cwd` and namespace links;
- `replies/authenticated/`: each reply to rastro's fixed list of commands, the bytes the server
  sent.

Only what a test reads is kept, and the socket tables keep their listening rows only, the only
ones rastro reads.
