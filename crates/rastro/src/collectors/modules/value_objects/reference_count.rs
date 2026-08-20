//! How many references the kernel holds to a module.

use rastro_collector::Observation;

/// A module's use count, or the fact that this kernel does not track one.
///
/// An enum rather than an `Option`, because the absent case has a reason worth
/// naming: a kernel built without `CONFIG_MODULE_UNLOAD` cannot know the count. "Nobody
/// is using this module" and "this kernel does not count" are different facts and must
/// not render alike.
///
/// A negative count is legal: the kernel reports `-1` while a module is unloading.
///
/// How an interface *spells* the untracked case is not this type's business, so there
/// is no `parse` here. That sentinel belongs to the source that reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceCount {
    Counted(i64),
    NotTracked,
}

impl From<&ReferenceCount> for Observation {
    fn from(count: &ReferenceCount) -> Self {
        match count {
            ReferenceCount::Counted(count) => Observation::integer(*count),
            ReferenceCount::NotTracked => Observation::null(),
        }
    }
}
