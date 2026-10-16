//! Which of the two server families a process or a binary belongs to.

/// Redis, or the Valkey fork of it.
///
/// **Both, because Debian 13 and Alpine package them side by side** as independent servers,
/// neither standing in for the other, so a box can run one of each. Told apart by the program
/// name, which `/proc/<pid>/comm` keeps for the life of the process while the argument vector
/// is overwritten with a title. Once connected, Valkey still reports a `redis_version`, frozen
/// at the release it forked from, so the name is the only honest discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServerKind {
    Redis,
    Valkey,
}

impl ServerKind {
    /// Every family, for a caller that has to look for each.
    pub const ALL: [ServerKind; 2] = [ServerKind::Redis, ServerKind::Valkey];

    /// The server's program name, which is also what the kernel records as its `comm`.
    pub fn program(self) -> &'static str {
        match self {
            ServerKind::Redis => "redis-server",
            ServerKind::Valkey => "valkey-server",
        }
    }

    /// The family a program name belongs to, or nothing for any other program.
    pub fn from_program(program: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.program() == program)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ServerKind::Redis => "redis",
            ServerKind::Valkey => "valkey",
        }
    }
}
