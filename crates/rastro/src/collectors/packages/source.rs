//! Where the package list comes from, and how each manager spells it.

mod apk_database;
mod dpkg_query;
mod package_source;

pub use apk_database::ApkDatabase;
pub use dpkg_query::DpkgQuery;
pub use package_source::PackageSource;
