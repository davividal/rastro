//! The crate's headline promise, as a test.
//!
//! Every path here resolves through `rastro_collector`. If writing a collector
//! ever needs a `rastro_fingerprint::` import, this file stops compiling, which
//! is the only way that claim in the crate docs stays true.

use rastro_collector::{
    ClaimedReading, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, Content, FacetName, FilesystemClaim, FingerprintError, Observation, Presence,
    Scalar, Sensitivity, View, Volatility, WalkedTree, Xxh3Digest,
};

struct OutOfTreeCollector {
    name: FacetName,
    identity: CollectorIdentity,
}

impl OutOfTreeCollector {
    fn new() -> Result<Self, FingerprintError> {
        Ok(Self {
            name: FacetName::new("out-of-tree")?,
            identity: CollectorIdentity::new(
                CollectorId::new("out-of-tree")?,
                CollectorVersion::new("0.1.0")?,
            ),
        })
    }
}

impl Collector for OutOfTreeCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::object([
            ("stable", Observation::text("value")),
            ("changes", Observation::integer(1).volatile()),
            ("secret", Observation::text("hunter2").sensitive()),
            // An author who hashes something reaches the one digest spelling through this
            // crate, or invents a second one for the same document.
            ("digest", (&Xxh3Digest::of(b"observed")).into()),
        ]))
    }

    /// A collector that owns a tree says so through this crate too, or an author would have
    /// to reach past it to claim one.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        let tree = WalkedTree::new("/var/lib/out-of-tree").expect("a legal tree");

        vec![FilesystemClaim::sealed(tree)]
    }
}

#[test]
fn a_collector_can_be_written_against_this_crate_alone() {
    // Arrange
    let collector = OutOfTreeCollector::new().expect("these identifiers are legal");

    // Act
    let observation = collector.collect().expect("this stub cannot fail");

    // Assert
    assert_eq!(collector.name().as_str(), "out-of-tree");
    assert_eq!(collector.presence(), Presence::Present);
    assert_eq!(collector.category(), CollectorCategory::State);

    // The annotation surface is what a real collector reaches for most, so the
    // guard has to name it. Without these the re-exports could be deleted and
    // this file would still compile.
    let Content::Object(entries) = observation.content() else {
        panic!("expected an object observation");
    };
    assert_eq!(entries["stable"].volatility(), Volatility::Stable);
    assert_eq!(entries["changes"].volatility(), Volatility::Volatile);
    assert_eq!(entries["secret"].sensitivity(), Sensitivity::Sensitive);
    assert_eq!(
        entries["stable"].content(),
        &Content::Scalar(Scalar::Text("value".to_owned()))
    );

    // A collector that reports which view produced the document needs `View`
    // too, so the re-export is guarded like the rest.
    assert!(observation.in_view(View::Diffable).is_some());

    // The claim surface, for the same reason: a collector that owns a tree reaches it
    // through this crate or not at all.
    let claims = collector.filesystem_claims();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].tree().as_str(), "/var/lib/out-of-tree");
    assert_eq!(claims[0].reading(), ClaimedReading::Sealed);
}
