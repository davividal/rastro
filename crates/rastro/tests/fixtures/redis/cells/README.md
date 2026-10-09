# Captured redis and valkey cells

What each cell of `docs/redis-matrix.md` showed, captured on real servers by the matrix tooling in
rastro-research (`collectors/redis-expert/matrix/`). Regenerate rather than edit.

Per cell: the host's own socket tables (`host-net/`), mount table and namespace links, which is
what rastro compares each server's against. Per server, `server-N/`:

- `process/`: the server's `cmdline`, `comm`, `stat`, `cgroup`, `mountinfo`, its own socket
  tables, its descriptors as `fd.list`, and the targets of its `cwd` and namespace links;
- `unit`, `unit.show`: the unit its cgroup names, and what `systemctl show` said of it;
- `files/`: the files the cell configured, read inside the server's own root at the same paths;
- `replies/authenticated/`: each reply to rastro's fixed list of commands, the bytes the server
  sent, after `AUTH` where the cell has a password.

Only what a test reads is kept, and three edits are made on export, none of which changes what
rastro reads: blank and comment lines are dropped from every `.conf`, which the server and rastro
both skip; the socket tables keep their listening rows only, the only ones rastro reads; and a
`CONFIG GET *` reply is kept once per release and shape, not for every cell whose server answers
exactly as another's. What each cell cost the server, and rastro's own reading of it, are recorded
in the matrix document rather than here. Every password is invented for the matrix.
