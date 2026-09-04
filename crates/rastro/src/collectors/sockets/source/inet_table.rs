//! Which of the four internet socket tables is being read.

/// One of `/proc/net/{tcp,tcp6,udp,udp6}`.
///
/// **The table decides how its own rows are read**, and all three of those decisions differ
/// between the four files: how wide the address field is, which state means "this is a
/// socket the box is offering", and what to call that state in the document. Passing the
/// filename around and branching on it at each site would put those three decisions in
/// three places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InetTable {
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

impl InetTable {
    /// Every table rastro reads, with the file each lives in.
    pub const ALL: [(Self, &str); 4] = [
        (Self::Tcp, "tcp"),
        (Self::Tcp6, "tcp6"),
        (Self::Udp, "udp"),
        (Self::Udp6, "udp6"),
    ];

    /// Whether addresses here are 16 bytes rather than 4.
    pub fn is_ipv6(&self) -> bool {
        matches!(self, Self::Tcp6 | Self::Udp6)
    }

    /// The protocol word this table's rows carry into the document.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Tcp | Self::Tcp6 => "tcp",
            Self::Udp | Self::Udp6 => "udp",
        }
    }

    /// The `st` value that means the box is offering this socket.
    ///
    /// `0A` is `TCP_LISTEN`. `07` is `TCP_CLOSE`, which is what an unconnected UDP socket
    /// sits in: UDP has no listen state, and a bound-but-unconnected datagram socket is
    /// exactly the open port an operator cares about. A *connected* UDP socket is in `01`
    /// and is traffic rather than state, so it is left out for the same reason an
    /// established TCP connection is.
    pub fn offered_state(&self) -> &'static str {
        match self {
            Self::Tcp | Self::Tcp6 => "0A",
            Self::Udp | Self::Udp6 => "07",
        }
    }

    /// What that state is called in the document.
    ///
    /// The words `ss` uses, kept deliberately: the source changed, and the vocabulary a
    /// reader has to learn should not change with it.
    pub fn state_word(&self) -> &'static str {
        match self {
            Self::Tcp | Self::Tcp6 => "LISTEN",
            Self::Udp | Self::Udp6 => "UNCONN",
        }
    }
}
