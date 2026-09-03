//! The `nginx -V` interface.
//!
//! The banner and the configure line, printed to **stderr**, exiting zero. Measured on
//! nginx 1.30: nothing at all arrives on stdout, so a collector reading only that stream
//! would report a host with nginx as a host whose nginx will not say what it is.
//!
//! `-V` is the one nginx invocation that opens no configuration, which is why it is the one
//! this collector runs. `-t` and `-T` load the configuration to test it, and loading it
//! creates every log file it names that does not exist yet.

use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::nginx::model::Binary;
use crate::collectors::nginx::value_objects::{BuildVersion, ConfigureArgument};

const PROGRAM: &str = "nginx";

/// Print the banner and the configure line, and exit.
const VERSION_FLAG: &str = "-V";

const VERSION_BANNER: &str = "nginx version: ";
const COMPILER_BANNER: &str = "built by ";
const TLS_BANNER: &str = "built with ";
const ARGUMENTS_BANNER: &str = "configure arguments: ";

/// What separates the product from its version: `nginx/1.30.4`.
const PRODUCT_SEPARATOR: char = '/';

/// What the shell would have stripped from an argument holding spaces.
const ARGUMENT_QUOTE: char = '\'';

/// nginx as a tool that can be asked about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NginxBinary {
    tool: CanonicalTool,
}

impl NginxBinary {
    /// Finds nginx, or reports that this host has none installed.
    ///
    /// Absence here is genuine state rather than a gap in rastro's reach, which is why the
    /// collector answers `absent` on it: a box with no nginx in a system directory is a box
    /// that serves nothing with nginx.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn read(&self) -> Result<Binary, CollectionError> {
        let output = self.tool.run_capturing_stderr(&[VERSION_FLAG])?;
        let banner = format!("{}\n{}", output.stdout, output.stderr);

        parse(&banner, self.tool.path())
    }
}

/// Reads a banner into the binary it describes.
///
/// Separate from running the tool so the shapes other builds print can be exercised without
/// one of those builds being installed.
pub fn parse(banner: &str, path: &Path) -> Result<Binary, CollectionError> {
    let announced = after(banner, VERSION_BANNER).ok_or_else(|| {
        CollectionError::new(format!(
            "no line of nginx's own banner begins {VERSION_BANNER:?}, so what it printed is \
             not a version: {banner:?}"
        ))
    })?;

    let (product, version) = announced.split_once(PRODUCT_SEPARATOR).ok_or_else(|| {
        CollectionError::new(format!(
            "nginx announced itself as {announced:?}, which carries no \
             {PRODUCT_SEPARATOR:?} to tell the product from the version"
        ))
    })?;

    Ok(Binary {
        path: AbsolutePath::new(path.to_string_lossy(), "nginx binary")?,
        product: NonEmptyText::new(product, "nginx product name")?,
        version: BuildVersion::new(version)?,
        compiler: optional(banner, COMPILER_BANNER, "nginx compiler")?,
        tls_library: optional(banner, TLS_BANNER, "nginx TLS library")?,
        configure_arguments: arguments(banner)?,
    })
}

/// The rest of the line that begins with `anchor`, trimmed.
///
/// Trimmed because nginx's own `built by` line ends in a space, and a trailing space that
/// reached the document would be invisible in a diff and present in the bytes.
fn after<'a>(banner: &'a str, anchor: &str) -> Option<&'a str> {
    banner
        .lines()
        .find_map(|line| line.trim().strip_prefix(anchor))
        .map(str::trim)
}

/// A banner line that not every build prints.
///
/// A binary built without TLS prints no `built with` line at all, and that is state rather
/// than a failure: the facet records nothing there rather than refusing the whole read.
fn optional(
    banner: &str,
    anchor: &str,
    kind: &str,
) -> Result<Option<NonEmptyText>, CollectionError> {
    after(banner, anchor)
        .filter(|value| !value.is_empty())
        .map(|value| NonEmptyText::new(value, kind))
        .transpose()
}

/// The configure line, split the way a shell would have joined it.
///
/// Whitespace separates arguments except inside single quotes, which is how nginx prints the
/// two that hold a whole compiler command line. Splitting on every space would report
/// `-O2` as a configure argument of its own.
fn arguments(banner: &str) -> Result<Vec<ConfigureArgument>, CollectionError> {
    let Some(line) = after(banner, ARGUMENTS_BANNER) else {
        return Ok(Vec::new());
    };

    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for character in line.chars() {
        match character {
            ARGUMENT_QUOTE => quoted = !quoted,
            _ if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    arguments.push(ConfigureArgument::new(std::mem::take(&mut current))?);
                }
            }
            _ => current.push(character),
        }
    }

    if !current.is_empty() {
        arguments.push(ConfigureArgument::new(current)?);
    }

    Ok(arguments)
}
