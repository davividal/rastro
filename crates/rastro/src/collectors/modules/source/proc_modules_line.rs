//! One line of `/proc/modules`, as the kernel wrote it.
//!
//! Everything peculiar to this interface lives here. It is less regular than it
//! looks, and the irregularities were read off `kernel/module/procfs.c` rather than
//! guessed:
//!
//! - The reference-count column is `-` on a kernel built without
//!   `CONFIG_MODULE_UNLOAD`, whose `print_unload_info` is a stub writing `" - -"`.
//! - The dependants column is one whitespace-free token, because the refcount is
//!   printed as `" %i "` with a trailing space and each dependant as `"%s,"` with
//!   none. It carries a trailing comma so the format is distinguishable from an older
//!   one, and `-` when there are no dependants.
//! - `[permanent]` is an element *of that column*, not a column, and means the module
//!   has an init and no exit so it can never be removed.
//! - The taint column exists only when the module is tainted, and `module_flags`
//!   wraps the letters in parentheses and may append `-` or `+` for a module going or
//!   coming.

use rastro_collector::{ByteSize, CollectionError};

use crate::collectors::modules::model::KernelModule;
use crate::collectors::modules::value_objects::{
    Dependants, ModuleName, ModuleState, ReferenceCount, Removability, TaintFlag, TaintFlags,
};

/// The element of the dependants column that is not a module.
const PERMANENT: &str = "[permanent]";

/// The column value meaning "nothing here", used for both dependants and, on a kernel
/// that cannot count them, the reference count.
const NOTHING: &str = "-";

/// The columns of one line that rastro reads.
///
/// The kernel writes six, or seven for a tainted module. The load address is dropped
/// at this boundary: it changes every boot and it is a kernel text pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcModulesLine {
    name: String,
    size: String,
    reference_count: String,
    dependants: String,
    state: String,
    taints: Option<String>,
}

impl ProcModulesLine {
    /// Splits one line into its columns.
    ///
    /// A line that is not what this interface promises is refused rather than skipped:
    /// reporting a table rastro half-understood as complete is the one failure this
    /// project will not accept.
    /// Whitespace is a safe separator here, unlike in the mount table: every column the
    /// kernel writes for a module is generated ASCII, so no unescaped Unicode space can reach
    /// it from a name a user chose.
    pub fn parse(line: &str) -> Result<Self, CollectionError> {
        let columns: Vec<&str> = line.split_whitespace().collect();

        let (name, size, reference_count, dependants, state, taints) = match columns.as_slice() {
            [name, size, reference_count, dependants, state, _address] => {
                (name, size, reference_count, dependants, state, None)
            }
            [
                name,
                size,
                reference_count,
                dependants,
                state,
                _address,
                taints,
            ] => (
                name,
                size,
                reference_count,
                dependants,
                state,
                Some(*taints),
            ),
            _ => {
                return Err(CollectionError::new(format!(
                    "expected six columns in a /proc/modules line, or seven when tainted, got {}: {line:?}",
                    columns.len()
                )));
            }
        };

        Ok(Self {
            name: (*name).to_owned(),
            size: (*size).to_owned(),
            reference_count: (*reference_count).to_owned(),
            dependants: (*dependants).to_owned(),
            state: (*state).to_owned(),
            taints: taints.map(str::to_owned),
        })
    }

    /// Translates the kernel's spelling into rastro's model, paired with the name it
    /// is filed under.
    ///
    /// A pair rather than a named type because it is a map entry, which is the shape
    /// [`ModuleTable`](crate::collectors::modules::model::ModuleTable) is built from.
    pub fn to_entry(&self) -> Result<(ModuleName, KernelModule), CollectionError> {
        let name = ModuleName::new(&self.name)?;
        let reference_count = self.to_reference_count()?;
        let module = KernelModule {
            size: self.to_size()?,
            state: ModuleState::parse(&self.state)?,
            dependants: self.to_dependants()?,
            removability: self.to_removability(reference_count),
            taints: self.to_taints()?,
            reference_count,
        };

        Ok((name, module))
    }

    /// Reads the count, or the sentinel this interface uses when the kernel cannot
    /// count.
    fn to_reference_count(&self) -> Result<ReferenceCount, CollectionError> {
        if self.reference_count == NOTHING {
            return Ok(ReferenceCount::NotTracked);
        }

        let count = self.reference_count.parse::<i64>().map_err(|_| {
            CollectionError::new(format!(
                "{:?} is not a module reference count",
                self.reference_count
            ))
        })?;

        Ok(ReferenceCount::Counted(count))
    }

    fn to_size(&self) -> Result<ByteSize, CollectionError> {
        let bytes = self.size.parse::<u64>().map_err(|_| {
            CollectionError::new(format!("{:?} is not a module size in bytes", self.size))
        })?;

        ByteSize::new(bytes, "module size")
    }

    fn to_dependants(&self) -> Result<Dependants, CollectionError> {
        let dependants = self
            .dependant_elements()
            .filter(|element| *element != PERMANENT)
            .map(ModuleName::new)
            .collect::<Result<Vec<ModuleName>, CollectionError>>()?;

        Ok(Dependants::new(dependants))
    }

    /// Reads removability, or the fact that this kernel cannot answer.
    ///
    /// Derived from the reference count rather than by re-reading the sentinel, so the one fact
    /// the two share is expressed once: `print_unload_info` is compiled out with
    /// `CONFIG_MODULE_UNLOAD`, and the same stub that leaves the count untracked is the one that
    /// makes `[permanent]` unprintable. A missing marker there means "unknowable", not
    /// "removable".
    fn to_removability(&self, reference_count: ReferenceCount) -> Removability {
        if reference_count == ReferenceCount::NotTracked {
            return Removability::Unknown;
        }

        if self
            .dependant_elements()
            .any(|element| element == PERMANENT)
        {
            return Removability::Permanent;
        }

        Removability::Removable
    }

    /// The comma-separated elements of the dependants column.
    ///
    /// The trailing comma leaves an empty final element, and `-` stands for none, so
    /// both are dropped here rather than in each caller.
    fn dependant_elements(&self) -> impl Iterator<Item = &str> {
        self.dependants
            .split(',')
            .filter(|element| !element.is_empty() && *element != NOTHING)
    }

    fn to_taints(&self) -> Result<TaintFlags, CollectionError> {
        let Some(column) = &self.taints else {
            return Ok(TaintFlags::default());
        };

        let letters = column
            .strip_prefix('(')
            .and_then(|column| column.strip_suffix(')'))
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "expected a parenthesised taint column, got {column:?}"
                ))
            })?;

        // `-` and `+` mark going or coming, which the state column already reports.
        let flags = letters
            .chars()
            .filter(|letter| *letter != '-' && *letter != '+')
            .map(TaintFlag::from_letter);

        Ok(TaintFlags::new(flags))
    }
}
