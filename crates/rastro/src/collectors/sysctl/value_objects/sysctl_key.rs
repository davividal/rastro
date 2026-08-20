//! What a kernel parameter is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// A kernel parameter's name, in the dotted spelling an operator recognises.
///
/// `net.ipv4.ip_forward`, not the path the kernel publishes it under. The dotted
/// form is the sysctl vocabulary's own identifier: it is what `sysctl -w` takes,
/// what a `sysctl.d` drop-in writes, and therefore what an operator scans a diff
/// for.
///
/// **The dot and the slash trade places, and that is a bijection rather than a
/// mangling.** A parameter's name can itself contain a dot: this repository's
/// own test box publishes `fs/binfmt_misc/python3.11`, and a VLAN interface
/// gives `net/ipv4/conf/eth0.100/forwarding`. Joining those segments naively on
/// `.` produces `fs.binfmt_misc.python3.11`, from which nobody can tell which
/// dot separates a segment and which belongs to a name. So a dot *inside* a
/// segment is written as a slash, exactly as procps' `sysctl` has always done,
/// giving `fs.binfmt_misc.python3/11`. Swapping the two characters back recovers
/// the original path, so no information is lost either way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SysctlKey(NonEmptyText);

/// The parameters that move on their own, so that a diff of an unchanged host
/// stays empty.
///
/// **Six of these were measured rather than assumed.** Snapshots of every
/// readable parameter on an idle Debian 12 box, twenty seconds apart and again
/// under forty parallel directory walks, disagreed on exactly these six.
/// `kernel.random.uuid` is the extreme case and the one that would have broken
/// the determinism harness by itself: the kernel mints a fresh UUID on *every
/// read*, so two reads within a single run already disagree.
///
/// **The rest are here by construction, not by measurement**, and the difference is
/// the point of this comment. None of them moved in that window: the entropy pool was
/// saturated, nobody logged in, the box uses no disk quotas, nothing was using
/// asynchronous I/O, and it has no conntrack module loaded at all. Every one of them is
/// nevertheless a counter the kernel maintains rather than a setting anyone chose, so a
/// box doing the work they count moves them.
///
/// Marking a settled parameter volatile costs a little signal; missing a moving one costs
/// the byte-identical diffable view that the whole tool rests on, and nothing but the
/// determinism harness would ever say so. Here that asymmetry is not even a trade: a
/// count of quota cache hits has no signal to lose.
///
/// **This list is inherently incomplete, and pretending otherwise is the trap.** It was
/// extended once already, after CI caught a `sysctl` divergence that the development box
/// could not reproduce — the box has no `nf_conntrack_count` because it loads no conntrack
/// module, while a CI runner with a container engine does, and it changes with every
/// connection the runner opens. There is no structural way to tell a counter from a
/// setting: mode `0444` looked promising and marks `kernel.osrelease` too, which would be
/// a disastrous thing to drop from a diff. So the list is a list, the determinism harness
/// is the backstop, and a future addition is expected rather than a failure.
const SELF_CHANGING: [&str; 19] = [
    "fs.aio-nr",
    "fs.dentry-state",
    "fs.file-nr",
    "fs.inode-nr",
    "fs.inode-state",
    "fs.quota.allocated_dquots",
    "fs.quota.cache_hits",
    "fs.quota.drops",
    "fs.quota.free_dquots",
    "fs.quota.lookups",
    "fs.quota.reads",
    "fs.quota.syncs",
    "fs.quota.warnings",
    "fs.quota.writes",
    "kernel.ns_last_pid",
    "kernel.pty.nr",
    "kernel.random.entropy_avail",
    "kernel.random.uuid",
    "net.netfilter.nf_conntrack_count",
];

/// Parameters whose value is a key rather than a setting.
///
/// `tcp_fastopen_key` signs TCP Fast Open cookies, and it reads back in full as
/// root. Note what is deliberately *not* here: whether the parameter is set at
/// all is not itself a secret, and that distinction survives redaction because
/// a parameter the kernel refuses to report is recorded as null rather than as a
/// redacted value.
const SECRET_PARAMETERS: [&str; 1] = ["net.ipv4.tcp_fastopen_key"];

/// Secrets that exist once per network interface, so no full name can list them.
///
/// `net.ipv6.conf.<interface>.stable_secret` is the per-interface seed RFC 7217
/// addresses are derived from, and the interface names on a host are not known
/// ahead of time. Matching the last segment is what covers `lo`, `enp0s8` and an
/// interface nobody has created yet alike.
const SECRET_LEAVES: [&str; 1] = ["stable_secret"];

impl SysctlKey {
    /// The name these path segments spell, with dots and slashes traded.
    ///
    /// Takes segments rather than a path because the translation is a rule of the
    /// sysctl vocabulary, not of the filesystem that happens to publish it: a
    /// second interface reporting the same parameters would still have to obey it.
    ///
    /// An empty segment is refused. The kernel publishes no nameless directory, so
    /// one means the caller split a path that had a double separator or a trailing
    /// one, and a key with a `..` in the middle of it would be a silent misread
    /// rather than a parameter.
    pub fn of<S: AsRef<str>>(segments: &[S]) -> Result<Self, CollectionError> {
        if segments.is_empty() {
            return Err(CollectionError::new(
                "a sysctl key needs at least one segment",
            ));
        }

        let mut name = String::new();
        for segment in segments {
            let segment = segment.as_ref();
            if segment.is_empty() {
                return Err(CollectionError::new(
                    "a sysctl key cannot have an empty segment",
                ));
            }

            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(&segment.replace('.', "/"));
        }

        Ok(Self(NonEmptyText::new(name, "sysctl key")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this parameter changes without anybody changing the host.
    ///
    /// A judgement about the name, because nothing in the value betrays it: `864`
    /// gives no hint that it is the current count of open file descriptors.
    pub fn changes_on_its_own(&self) -> bool {
        SELF_CHANGING.contains(&self.as_str())
    }

    /// Whether this parameter's value must not be printed as it stands.
    pub fn holds_a_secret(&self) -> bool {
        SECRET_PARAMETERS.contains(&self.as_str()) || SECRET_LEAVES.contains(&self.last_segment())
    }

    /// The name's final segment, which is what a per-interface parameter is
    /// recognised by.
    fn last_segment(&self) -> &str {
        self.as_str()
            .rsplit('.')
            .next()
            .expect("`rsplit` yields at least one segment")
    }
}
