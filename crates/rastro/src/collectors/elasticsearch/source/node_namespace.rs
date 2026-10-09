//! The network namespace a node listens in, and doing work inside it: the shared
//! [`ProcessNamespace`], its refusals said as this facet says what it could not read.

use std::path::Path;

use crate::collectors::elasticsearch::value_objects::Unread;
use crate::collectors::network_namespace::{NamespaceRefusal, ProcessNamespace};

/// The network namespace a node listens in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeNamespace(ProcessNamespace);

impl NodeNamespace {
    /// A node's namespace through a process table the caller names, compared with its `self`.
    pub fn of_in(proc: &Path, process_id: u32) -> Result<Self, Unread> {
        ProcessNamespace::of_in(proc, process_id)
            .map(Self)
            .map_err(|refusal| Unread::new(refusal.reason))
    }

    /// Whether the node listens in the namespace rastro runs in, which is a node on the host.
    pub fn is_ours(&self) -> bool {
        self.0.is_ours()
    }

    /// Does `work` inside the node's namespace, and does not do it at all where that fails.
    ///
    /// A join the box refuses is `not_read`, which "Not read is not an error" makes it: an
    /// unprivileged run has no `CAP_SYS_ADMIN`, found by review.
    pub fn run<Answer: Send>(
        &self,
        work: impl FnOnce() -> Answer + Send,
    ) -> Result<Answer, Unread> {
        self.0.run(work).map_err(unread_of)
    }
}

fn unread_of(refusal: NamespaceRefusal) -> Unread {
    match refusal.refused {
        true => Unread::not_read(refusal.reason),
        false => Unread::new(refusal.reason),
    }
}
