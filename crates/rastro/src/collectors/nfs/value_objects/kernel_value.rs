//! A value the kernel publishes as a file.

use rastro_collector::Observation;

/// One file's value, or why there is none.
///
/// Three answers that diff three ways. An empty value is ordinary, a parameter never set, and
/// stays the empty text it is. A file this kernel does not publish is `null`. A file this run
/// could not read is an error, never a `null`, which would make an unprivileged run read as a
/// kernel lacking the setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelValue {
    Reported(String),
    Unpublished,
    Unreadable(String),
}

impl KernelValue {
    /// The value as the kernel wrote it, less the one newline that ends it.
    ///
    /// Only the last: `portlist` spans a line per listener, and those lines are the value.
    pub fn reported(written: impl Into<String>) -> Self {
        let mut written = written.into();
        if written.ends_with('\n') {
            written.pop();
        }

        Self::Reported(written)
    }
}

impl From<&KernelValue> for Observation {
    fn from(value: &KernelValue) -> Self {
        match value {
            KernelValue::Reported(value) => Observation::text(value.clone()),
            KernelValue::Unpublished => Observation::null(),
            KernelValue::Unreadable(reason) => {
                Observation::object([("error", Observation::text(reason.clone()))]).incomplete()
            }
        }
    }
}
