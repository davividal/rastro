//! Layer 3: what a PostgreSQL cluster is actually running with.
//!
//! The walk from a Layer 2 signal to service-internal state, and the first collector to
//! read a service rather than the kernel or the package manager.
//!
//! **The server's own view, not `postgresql.conf`.** A cluster's effective configuration
//! is not what the file says: an `ALTER SYSTEM`, a command-line override or a value the
//! build defaults to are all invisible in the file, and a file edited without a reload is
//! a value the server is not using. `pg_settings` answers all of that in one place, and
//! carries the two columns a file never can: where each value came from, and whether the
//! running server has taken it up yet.
//!
//! The collector itself is not here yet. This is the interface and the meaning: reading
//! psql's output into a cluster's configuration.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{ClusterSettings, Setting};
pub use source::PsqlSettings;
pub use value_objects::{SettingName, SettingSource, SettingUnit, SettingValue};
