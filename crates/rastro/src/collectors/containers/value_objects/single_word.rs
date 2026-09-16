//! The rule every name an engine reports has to keep.

use rastro_collector::{CollectionError, NonEmptyText};

/// A name the engine gave, refused if it holds whitespace.
///
/// **One rule in one place, because it is one rule.** A container, a volume, a network, a
/// label key and a containerd namespace are all named by a single word, and a value with a
/// space in it means the answer was split in the wrong place rather than that the operator
/// chose a name with a space: none of the engines will accept one. Recorded anyway it would
/// key an entry under half of somebody else's field.
///
/// The `subject` is what the failure calls the value, so the reader of a fingerprint's error
/// learns which list was misread.
pub fn single_word(
    value: impl Into<String>,
    subject: &str,
) -> Result<NonEmptyText, CollectionError> {
    let text = NonEmptyText::new(value, subject)?;

    if text.as_str().chars().any(char::is_whitespace) {
        return Err(CollectionError::new(format!(
            "the host reported the {subject} {:?}, and one holding whitespace means the \
             answer was misread",
            text.as_str()
        )));
    }

    Ok(text)
}
