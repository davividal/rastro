//! One token a directive was given.

/// An argument as nginx read it, with its quoting already spent.
///
/// **Empty is a value here**, which is why this is the one field in the facet that does not
/// go through [`NonEmptyText`](rastro_collector::NonEmptyText): `add_header X-Empty "";`
/// passes nginx an empty argument, and a directive that lost it would read as a different
/// directive.
///
/// The quotes themselves are gone by the time a value reaches this type. They are spelling
/// rather than state: `proxy_pass "http://pool";` and `proxy_pass http://pool;` configure
/// the same thing, and a fingerprint that told them apart would report a requoting as a
/// change to the service.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirectiveArgument(String);

impl DirectiveArgument {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
