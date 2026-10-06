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
