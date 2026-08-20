//! What the account database says an account is for.

use rastro_collector::Observation;

/// An account's comment field, whole and unsplit.
///
/// **Deliberately not split on its commas.** The field conventionally holds four
/// comma-separated subfields, a full name and then an office, a work telephone
/// number and a home one, and `chfn` writes them that way. It is a convention of
/// `finger` rather than a rule of the file: nothing enforces the count, plenty of
/// accounts hold one comma-free phrase, and Debian's own `postgres` account reads
/// `PostgreSQL administrator,,,`. Splitting would invent three empty fields there
/// and mis-slot any comment that simply contains a comma.
///
/// **Deliberately not empty-checked either.** Most system accounts have no comment
/// at all, so a blank one is ordinary rather than evidence of a misread. That is why
/// this is not built on
/// [`NonEmptyText`](rastro_collector::NonEmptyText) like most values here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Comment(String);

impl Comment {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&Comment> for Observation {
    fn from(comment: &Comment) -> Self {
        Observation::text(comment.as_str())
    }
}
