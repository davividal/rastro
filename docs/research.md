# Prior art: server state fingerprinting and drift detection

The field research that preceded rastro. It settled *what* is worth capturing on
an undocumented host, and it established that nothing off the shelf does that
scope-complete with zero prior knowledge.

**Read this as provenance, not as an integration plan.** AIDE and configsnap
appear here as evaluated candidates and were **rejected as dependencies**;
rastro's collectors are native. See
[the decision](decisions.md#native-collectors-no-external-tool-as-a-dependency).
What survives from this research is the three-layer state model, the
effective-config-over-file-reads principle, the attribute set worth capturing,
the noise-floor discipline, and a catalogue of the ways the existing tools fail.

**Context:** a small Debian fleet, undocumented, being brought under
configuration management for the first time. Boxes of this kind typically carry
undocumented cron jobs, legacy shell scripts, and integrations nobody
remembers.

## Goal

Answer one question, repeatably, on a box you know nothing about:

```
generate-state > before
ansible-playbook whatever.yml
generate-state > after
diff before after
```

What a brand-new configuration-management role *actually* did to a live server,
not what the tool *says* it did.

**Explicitly out of scope:** preventing drift, remediating drift, continuous
monitoring, agents, central servers. This is a sparse, deliberate,
operator-invoked diagnostic.

## The hard requirement that disqualified most tooling

**Scope-complete with zero prior knowledge.**

Any tool that requires you to declare *what to watch* is disqualified by the
premise: if you could enumerate what changes, you would not need the diff. That
rules out declared-collector approaches, including configsnap's `type=file`
entries.

## The three-layer model

State surfaces decompose into three layers. Only the third is service-specific,
and it is *derived*, never guessed.

### Layer 1: filesystem. Agnostic, complete, zero declaration

**Tool evaluated: AIDE.** `--init` builds a baseline, `--check` diffs against
it. It does not care what is running on the box; it hashes what is there.

Two distinct products from one tool, which is easy to conflate:

- **`--init` DB is an inventory.** Plain text, gzipped, greppable. This is the
  file-level documentation that does not otherwise exist, and it is available
  with no change and no event:
  ```sh
  zcat /var/lib/aide/aide.db | awk '{print $1}' | grep -E 'cron|\.wants/'
  ```
- **`--check` is drift.** Only reports what changed. Correct for drift, useless
  for discovery.

Tested configuration (AIDE 0.18.6):

```
database_in=file:/var/lib/aide/aide.db
database_out=file:/var/lib/aide/aide.db.new
database_new=file:/var/lib/aide/aide.db.new
gzip_dbout=yes
report_url=stdout
log_level=warning

# Group names MUST be CamelCase in 0.18 — lowercase parses as a config
# option and fails with "unknown config option".
Strong = p+u+g+s+m+c+i+n+l+ftype+sha256+acl+xattrs
Meta   = p+u+g+s+m+c+i+n+l+ftype     # attributes only, never reads file bytes

/etc                        Strong
/usr/local                  Strong
/opt                        Strong
/root                       Strong
/var/spool/cron             Strong
/etc/systemd                Strong
/usr/lib/systemd/system     Strong
/srv                        Meta

!/etc/mtab$
!/etc/adjtime$
!/etc/ld.so.cache$
!/var/lib/postgresql
```

**Measured cost** (43,909 entries, sha256 on all `Strong` paths):

| operation | cost |
| --- | --- |
| `--init` | 59s |
| `--check` | 32s |
| DB size | 1.9 MB gzipped |

Tune scope by moving large trees to `Meta`, which skips reading bytes, and write
**exclusion** lists rather than inclusion lists. Exclusions are safe to get
wrong; inclusions are not.

**Why attributes and not content hashing alone.** Two changes caught in testing
that a content hash would miss:

```
f = p.. .c...A.   : /etc/shells
 Perm : -rw-r--r-- | -rw-------          # perms + ACL changed, content identical

l.= ... mci.  .   : /etc/systemd/system/multi-user.target.wants/cron.service
                                          # symlink = service enablement
```

Attributes available: `perm uid gid size ctime mtime inode lcount linkname
ftype acl xattrs selinux e2fsattrs caps`, plus 11 hash algorithms. This is the
set rastro's Layer 1 walker captures.

**Gotchas:**

- `/var/spool/cron/crontabs` holds user crontabs and is a classic blind spot.
  Debian's stock configuration excludes much of `/var`. Declare it explicitly.
- `apt install aide` pulls in `aide-common`, which installs
  `/etc/cron.daily/dailyaidecheck`. Disable it. Manual runs only, no nightly job
  mailing root on a production box.

### Layer 2: kernel and OS runtime. Also agnostic

The set of non-file state surfaces is **fixed and short**, and does not grow
with the number of tenants on the box:

processes, listening sockets, established connections, systemd unit states
(loaded/active/enabled), timers, kernel modules, runtime sysctl, the
nftables/iptables ruleset, mounts, the package list, users and groups, container
state.

This is roughly what configsnap's defaults already collect (`ps`,
`systemdinit`, `packages`, listening services, network, mounts, `sysctl`,
`lsblk`), and it is the one part of that tool worth learning from. Do **not**
point it at directories: see the bug below.

### Layer 3: service-internal state. The only service-specific layer

**Derived from Layer 2 output, not guessed.** The first run on a box is the
discovery step:

```sh
systemctl list-units --type=service --state=running --no-legend --no-pager | awk '{print $1}'
ss -lntupH | awk '{print $5, $NF}'
docker ps --format '{{.Image}} {{.Names}}' 2>/dev/null
```

Then dispatch:

| tenant | one-command state dump |
| --- | --- |
| nginx | `nginx -T` |
| apache | `apachectl -S`; `apache2ctl -t -D DUMP_MODULES` |
| postgres | `pg_dumpall --globals-only`; `psql -c "SHOW ALL"` |
| mysql/mariadb | `mysqldump --no-data`; `SHOW GLOBAL VARIABLES` |
| rabbitmq | `rabbitmqctl export_definitions -` |
| redis | `redis-cli CONFIG GET '*'`; `ACL LIST` |
| docker | `docker inspect $(docker ps -aq)`; `volume ls`; `network ls` |
| haproxy | `haproxy -f /etc/haproxy/haproxy.cfg -c -V` |
| any unit | `systemctl show <unit>` |

All of them emit text on stdout, and all of them no-op on a box without that
tenant. That property is what makes one configuration work fleet-wide and grow
as new services are met. In rastro it becomes `detect()` plus the `absent`
status.

## Core principle: effective config over file reads

Prefer **resolved and effective** dumps over reading configuration files.
`nginx -T` resolves every `include`. `systemctl show` resolves every drop-in.
`sysctl -a` reflects runtime. `SHOW ALL` reflects `ALTER SYSTEM` plus CLI
overrides.

The two failure modes are different and both matter:

- **File changed, meaning did not.** Noise. The file hasher fires, the effective
  dump is identical. Ignore it.
- **Meaning changed, file did not.** The dangerous one, and the file hasher is
  silent. `sysctl -w` from a script, `ALTER SYSTEM`, a container started with no
  compose file, iptables rules loaded from `rc.local`. Only the effective dump
  catches these.

File hashing alone produces false positives. Effective dumps alone produce false
negatives. Both are needed.

## configsnap: verified bug and patch

Repository: <https://github.com/rackerlabs/configsnap>. A single Python file,
Apache 2.0, runs under python3, no install needed.

Good for Layer 2: curated agnostic collectors and a sensible volatile-versus-
diffed split (`dmesg`, `ps`, `sysctl`, `meminfo` are captured but excluded from
the diff). rastro keeps that idea as format metadata rather than tool
convention.

**Do not use `type=directory` on `/etc/` as shipped.** Verified failure:

`copy_file` opens in text mode (`open(r_filename, 'r')`). The first non-UTF-8
file in the walk raises `UnicodeDecodeError`, which the section loop catches
broadly and reports as:

```
Could not parse config file section etc_full: 'utf-8' codec can't decode byte 0xb5 ...
```

It blames the configuration file, then **silently abandons the rest of the
directory and exits 0.** Measured: **6 of 302 files captured**, with output that
looks clean. This is the real cause of open issue #124, unanswered since
November 2021.

One-line fix, verified to take it from 6 to 302 of 302:

```sh
sed -i "s/r_fd = open(r_filename, 'r')/r_fd = open(r_filename, 'r', errors='surrogateescape')/; \
        s/w_fd = open(w_filename, 'w')/w_fd = open(w_filename, 'w', errors='surrogateescape')/" configsnap
```

**Second gap, not cheaply patchable:** `copy_dir` skips symlinks
(`not os.path.islink(...)`). That was 445 entries under `/etc` on the test box,
including `/etc/systemd/system/*.wants/*` enablement symlinks, `/etc/rc*.d`, and
alternatives. This is why rastro treats symlinks as first-class entries.

**Security:** `type=directory` on `/etc` copies `/etc/shadow` and
`/etc/ssl/private/*` into the basedir at **0644**, verified. This is why rastro
creates output `0600` and hashes sensitive values by default.

**Verdict:** a useful reference for Layer 2 collector selection, not a
dependency. 37 stars, 18 forks, one open issue unanswered since 2021, and a
silent-truncation bug found in about 20 minutes of poking.

## Supporting techniques

These are operator techniques rather than rastro features, and they remain
useful alongside it.

**Whole-system file change catch-all**, for a window of minutes, with no
baseline needed:

```sh
touch /tmp/marker
ansible-playbook whatever.yml
find / -xdev -newerct /tmp/marker \
  -not -path "/proc/*" -not -path "/sys/*" -not -path "/run/*" \
  -not -path "/tmp/*" -not -path "/var/log/*" 2>/dev/null
```

`-newerct`, not `-newermt`: ctime catches `chmod` and `chown`, mtime does not.

**Attribution**, which is *who* changed it rather than only *what*:

```sh
fatrace -f W -o /tmp/trace.log &
ansible-playbook whatever.yml
kill %1
awk '{print $1, $2, $NF}' /tmp/trace.log | sort -u
```

fanotify-based and cheap, giving process name and PID per write. It answers "did
my role touch something it should not have" by direct observation rather than
inference, and it disambiguates an intended change from an undocumented cron job
that fired mid-window.

**Noise-floor calibration, before trusting any output.** Run the capture twice
back to back with no event in between, and diff the two. Whatever differs is
that box's volatile baseline. Filter it, or chase ghosts on every real run. It
takes two minutes and everyone skips it. rastro turns this into a CI test rather
than a ritual: see [the determinism harness](design.md#verification).

## Structural blind spot to respect

State diffing has a **time-horizon** limit. A change that breaks an undocumented
consumer produces a **clean** post-run diff, because nothing is wrong with the
state. It is wrong for a consumer that will not run until 03:00.

Mitigation is discovery *before* the change, not diffing after:

- enable connection or access logging on the service and let it run through a
  full cron cycle (at least 24 hours) to capture nightly jobs;
- search the box for callers and credentials:
  ```sh
  grep -rIl --exclude-dir=.git -e psql -e curl -e PGPASSWORD -e '://' \
    /etc/cron* /etc/systemd /usr/local/bin /usr/local/sbin /opt /srv /root /home 2>/dev/null
  find / -xdev -name '.pgpass' -o -name '.netrc' -o -name '*.env' 2>/dev/null
  ss -tnp state established
  ```
- prefer `reload` over `restart` in roles, since a restart drops live
  connections, including the integration nobody documented.

## Also evaluated and rejected

| option | why not |
| --- | --- |
| Puppet/Ansible `--diff` | reports only what the tool knows it touched; the premise is distrusting the tool |
| snapper / btrfs snapshots | works, near-zero create cost, but retention pins blocks proportional to write churn, which is dangerous on a database host over a long window. Also: never enable qgroups (transaction-commit slowdown, pathological snapshot deletes). Viable only for windows of minutes. |
| osquery / Fleet | a real schema, but ships no snapshot/diff harness; the daemon is a polling model rather than operator-defined event boundaries |
| driftctl / Terragrunt / Spacelift | IaC-state versus cloud-API drift. Wrong layer entirely. |
| etckeeper | good and cheap, but `/etc` only |
| GLPI / OCS / NetBox | ITAM/CMDB shape, not drift |

## Provenance

Everything marked "verified" or "measured" above was tested in an Ubuntu 24.04
container with AIDE 0.18.6 and configsnap from master, **not** on a production
host. Re-verify the timings and the entry count against a real host before
relying on the numbers. The configsnap bug and its patch were reproduced
directly and should hold anywhere python3 meets a non-UTF-8 file under a
`type=directory` rule.
