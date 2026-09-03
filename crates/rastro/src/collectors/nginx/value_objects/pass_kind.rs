//! Which protocol a location hands a request on with.

use rastro_collector::Observation;

/// The `*_pass` directives, one variant each.
///
/// An exhaustive enum rather than the directive's name as text: these are the five ways an
/// nginx forwards a request, and a sixth arriving should make the compiler name every place
/// that has to decide about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PassKind {
    Proxy,
    FastCgi,
    Uwsgi,
    Scgi,
    Grpc,
}

impl PassKind {
    /// The directive that spells it, which is also how the document names it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proxy => "proxy_pass",
            Self::FastCgi => "fastcgi_pass",
            Self::Uwsgi => "uwsgi_pass",
            Self::Scgi => "scgi_pass",
            Self::Grpc => "grpc_pass",
        }
    }

    /// The kind a directive name stands for, or nothing if it hands nothing on.
    pub fn of(directive: &str) -> Option<Self> {
        [
            Self::Proxy,
            Self::FastCgi,
            Self::Uwsgi,
            Self::Scgi,
            Self::Grpc,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == directive)
    }
}

impl From<&PassKind> for Observation {
    fn from(kind: &PassKind) -> Self {
        Observation::text(kind.as_str())
    }
}
