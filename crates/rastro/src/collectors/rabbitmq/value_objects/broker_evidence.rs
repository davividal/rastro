//! Why rastro believes a registered node is, or is not, a RabbitMQ broker.

/// What the box's own evidence says about one registered node.
///
/// **Three-valued at the top, like [`Presence`](rastro_collector::Presence), and for the same
/// reason.** A first version of this was a boolean, and a live broker in a capability-reduced
/// container reported `runs_rabbitmq: false`: rastro could not read the beam's descriptors, so
/// it could not see who held the distribution port, and a boolean had nowhere to put "could
/// not tell". A reader would have seen a confident denial about a broker that was plainly
/// running.
///
/// So the five host states are named, and each says which of the three answers it carries.
/// The two unreadable cases are the ones that matter: both mean rastro declines to address
/// the node, which is the safe behaviour either way, and now the document says why rather
/// than asserting something false about the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerEvidence {
    /// A process that booted RabbitMQ holds the node's distribution port.
    RabbitmqProcess,

    /// Something that did not boot RabbitMQ holds it, so this node belongs to another Erlang
    /// application and must not be addressed with a RabbitMQ CLI tool.
    OtherApplication,

    /// Nothing in a readable socket table offers that port, which is what a registration left
    /// behind by a node that is gone looks like.
    NotOffered,

    /// The port is offered and rastro could not see which process holds it.
    ///
    /// An unprivileged run cannot read another user's descriptors, and neither can a root in
    /// a container whose capabilities were reduced, which is where this case was found.
    HolderUnreadable,

    /// No socket table could be read at all, so the question was never answerable.
    TablesUnreadable,
}

impl BrokerEvidence {
    /// Whether this node runs RabbitMQ: `None` where the box could not say.
    /// The refusal this evidence is, where it is one, in the words the document reports.
    ///
    /// **Marked as a failure rather than merely described.** Every node carries
    /// [`Self::as_str`], healthy ones included, so a reader scanning for trouble sees a value
    /// and not a problem; `runs_rabbitmq: null` beside it is a tri-state doing its job and is
    /// no louder. An `error` key is how every other facet in this codebase spells "rastro was
    /// not allowed to look" — the walk does it per path — and a diff over a box nobody could
    /// read must not look like a diff over a box with no broker on it.
    pub fn refusal(&self) -> Option<&'static str> {
        match self {
            Self::HolderUnreadable => Some(
                "no descriptor naming this node's distribution socket could be read, so \
                 whether it is a broker is not something this run was allowed to find out",
            ),
            Self::TablesUnreadable => Some(
                "no socket table could be read, so what holds this node's distribution port \
                 is not something this run was allowed to find out",
            ),
            Self::RabbitmqProcess | Self::OtherApplication | Self::NotOffered => None,
        }
    }

    pub fn runs_rabbitmq(&self) -> Option<bool> {
        match self {
            Self::RabbitmqProcess => Some(true),
            Self::OtherApplication | Self::NotOffered => Some(false),
            Self::HolderUnreadable | Self::TablesUnreadable => None,
        }
    }

    /// Whether rastro may address this node.
    ///
    /// Only the confirmed case, which is the whole restraint: addressing another
    /// application's node makes it log an authentication failure, and addressing one rastro
    /// knows nothing about is the same gamble with less information.
    pub fn may_be_addressed(&self) -> bool {
        matches!(self, Self::RabbitmqProcess)
    }

    /// The evidence itself, for the document.
    pub fn as_str(&self) -> &str {
        match self {
            Self::RabbitmqProcess => "a process that booted rabbitmq holds the port",
            Self::OtherApplication => "another erlang application holds the port",
            Self::NotOffered => "no socket in the table offers the port",
            Self::HolderUnreadable => "the holder of the port could not be read",
            Self::TablesUnreadable => "no socket table could be read",
        }
    }
}
