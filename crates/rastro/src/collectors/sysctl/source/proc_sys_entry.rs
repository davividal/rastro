//! One entry under `/proc/sys`, as the kernel publishes it.
//!
//! The kernel's spelling, kept apart from rastro's meaning. Everything peculiar to
//! this interface lives here: a name spread across directory levels, a value
//! terminated by a newline that is punctuation rather than content, and a mode that
//! says whether the entry is a setting or a button.

use rastro_collector::CollectionError;

use crate::collectors::sysctl::value_objects::{Readability, SysctlKey, SysctlValue};

/// What one entry under the interface's root contributes to the facet.
///
/// A free function rather than a type, because there is no per-entry state to hold:
/// the interface hands over three independent facts about a file and this decides
/// what they mean together. Naming it after the interface keeps the mapping where
/// the rest of this interface's peculiarities are.
///
/// `Ok(None)` means the entry holds no state and belongs nowhere in the facet. That
/// is not the same as a failure and not the same as an absent value, and it is
/// reserved for the write-only triggers [`Readability`] describes.
///
/// **`reported` is `None` when the read itself failed**, which is a real and
/// expected state rather than a collection failure: the kernel answers `EIO` for an
/// unset `stable_secret`, on a file it advertises as readable. Recording that as
/// [`SysctlValue::Withheld`] keeps the parameter visible and keeps the run alive,
/// where treating it as a failure would cost the other twelve hundred parameters
/// over one that has simply never been set.
pub fn classify(
    segments: &[String],
    mode: u32,
    reported: Option<&[u8]>,
) -> Result<Option<(SysctlKey, SysctlValue)>, CollectionError> {
    if !Readability::of_mode(mode).holds_state() {
        return Ok(None);
    }

    let key = SysctlKey::of(segments)?;
    let value = match reported {
        Some(bytes) => SysctlValue::reported(decoded(&key, bytes)?),
        None => SysctlValue::Withheld,
    };

    Ok(Some((key, value)))
}

/// A parameter's value as text, or a refusal.
///
/// Invalid UTF-8 is refused rather than replaced with `U+FFFD`, for the same reason
/// the canonical-tool seam refuses it: substituting a character would put text into
/// a fingerprint that was never on the box. `kernel.core_pattern` is the parameter
/// that can genuinely hold arbitrary bytes, because whatever an operator wrote into
/// it is what reads back.
///
/// The failure names the parameter. A bare "not valid UTF-8" over twelve hundred
/// entries would leave an operator with nothing to grep for.
fn decoded(key: &SysctlKey, bytes: &[u8]) -> Result<String, CollectionError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        CollectionError::new(format!(
            "{} holds a value that is not valid UTF-8",
            key.as_str()
        ))
    })
}
