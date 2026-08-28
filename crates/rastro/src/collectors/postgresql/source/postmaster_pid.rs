//! Reading `postmaster.pid` into the observed half of a cluster.
//!
//! The layout is fixed by `src/include/utils/pidfile.h`: one value per line, in a defined
//! order. Only the stable lines are read, the port, the socket directory, the listen
//! addresses and the status. The PID (line 1) and the start time (line 3) are volatile and
//! never read; the shared memory key (line 7) is noise and left out.

use rastro_collector::CollectionError;

use crate::collectors::postgresql::model::Postmaster;
use crate::collectors::postgresql::value_objects::PostmasterStatus;

/// The lines, numbered as `pidfile.h` numbers them, that each field lives on.
const PORT_LINE: usize = 4;
const SOCKET_DIR_LINE: usize = 5;
const LISTEN_ADDR_LINE: usize = 6;
const PM_STATUS_LINE: usize = 8;

/// A `postmaster.pid` file, ready to be read as the observed cluster.
pub struct PostmasterPid;

impl PostmasterPid {
    /// Reads the fixed-layout pid file into the observed half.
    ///
    /// The port is required, because it is the reason the file is read at all; a file without
    /// it is too short to be a running postmaster's. The socket directory, listen addresses
    /// and status are each absent where the line is empty or the file stops before it.
    pub fn parse(content: &str) -> Result<Postmaster, CollectionError> {
        let lines: Vec<&str> = content.lines().collect();

        let port_line = line(&lines, PORT_LINE).ok_or_else(|| {
            CollectionError::new(
                "postmaster.pid has no port line, so the running port cannot be read",
            )
        })?;
        let port = port_line.parse::<u16>().map_err(|_| {
            CollectionError::new(format!(
                "postmaster.pid line {PORT_LINE} is {port_line:?}, which is not a port number"
            ))
        })?;

        Ok(Postmaster {
            port,
            socket_directory: present(line(&lines, SOCKET_DIR_LINE)),
            listen_addresses: present(line(&lines, LISTEN_ADDR_LINE)),
            status: match present(line(&lines, PM_STATUS_LINE)) {
                Some(status) => Some(PostmasterStatus::parse(&status)?),
                None => None,
            },
        })
    }
}

/// The line at `pidfile.h`'s own numbering, if the file is long enough to have it.
fn line<'a>(lines: &[&'a str], number: usize) -> Option<&'a str> {
    lines.get(number - 1).copied()
}

/// An empty line is an unset value.
fn present(line: Option<&str>) -> Option<String> {
    match line {
        Some(value) if !value.trim_end().is_empty() => Some(value.trim_end().to_owned()),
        _ => None,
    }
}
