//! The `ss` interface.

use rastro_collector::CollectionError;

use super::{ss_address, ss_users};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::sockets::model::{ListeningSocket, SocketAddress, SocketTable};
use crate::collectors::sockets::value_objects::{SocketKind, SocketPath, SocketState};

const PROGRAM: &str = "ss";

/// How many whitespace-separated fields an internet row has.
///
/// `netid state recv-q send-q local peer users`. Both `-t` and `-u` are always passed
/// together, and that is what guarantees the count: asked for one protocol only, `ss`
/// drops the `netid` column as redundant and every row becomes six fields.
const INET_FIELDS: usize = 7;

/// How many a unix row has.
///
/// `netid state recv-q send-q path inode peer-path peer-inode users`. Two more than an
/// internet row, because a unix socket is identified by an inode as well as a name, and
/// the peer columns are a constant `* 0` for a listener.
const LOCAL_FIELDS: usize = 9;

/// `ss`'s view of the listening sockets, as a source rastro can read.
///
/// # Why two runs rather than one
///
/// `ss` will report internet and unix sockets together, and asked to do so it prints two
/// different row shapes into one stream, seven fields and nine. A parser that had to tell
/// them apart by counting would be guessing, and it would guess wrong the moment a
/// process column went missing. So they are asked for separately and each is parsed by
/// the grammar it actually has.
///
/// # Why not `/proc/net/tcp`
///
/// It is the unambiguous source this project usually prefers, and it loses on a different
/// count: it writes addresses and ports as little-endian hexadecimal, and it names the
/// holder of a socket not at all. Finding that means walking every `/proc/<pid>/fd`
/// looking for the socket's inode, which is `ss`'s job reimplemented, on a moving target,
/// as root. `ss` is the canonical tool here in the full sense.
///
/// # Why not JSON
///
/// `ss` grew a `--json` option after the version Debian 12 ships; iproute2 6.1 rejects the
/// flag outright, which was checked on the box rather than assumed. When it is available
/// it will be a strictly better source, and the two column grammars are confined to their
/// own modules so that adding it is a new source rather than a change to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ss {
    tool: CanonicalTool,
}

impl Ss {
    /// Finds `ss`, or reports that this host does not have it.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// Asks for both socket families and joins the answers.
    ///
    /// `-H` drops the header, `-l` keeps only listeners, `-n` stops `ss` resolving ports
    /// into service names out of `/etc/services`, which would make the facet depend on a
    /// file rather than on the kernel, and `-p` adds the holding processes.
    pub fn read(&self) -> Result<SocketTable, CollectionError> {
        let inet = self.tool.run(&["-H", "-l", "-n", "-p", "-t", "-u"])?;
        let local = self.tool.run(&["-H", "-l", "-n", "-p", "-x"])?;

        Self::parse(&inet, &local)
    }

    /// Translates both outputs into the model.
    ///
    /// Separate from [`Self::read`] so both grammars are exercised from a fixture, with no
    /// `ss` to run.
    pub fn parse(inet: &str, local: &str) -> Result<SocketTable, CollectionError> {
        let mut sockets = Vec::new();

        for line in rows(inet) {
            sockets.push(Self::parse_inet(line)?);
        }
        for line in rows(local) {
            sockets.push(Self::parse_local(line)?);
        }

        Ok(SocketTable::new(sockets))
    }

    fn parse_inet(line: &str) -> Result<ListeningSocket, CollectionError> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [
            kind,
            state,
            _receive_queue,
            _send_queue,
            local,
            _peer,
            users @ ..,
        ] = fields.as_slice()
        else {
            return Err(malformed(line, INET_FIELDS, fields.len()));
        };

        Ok(ListeningSocket {
            kind: SocketKind::new(*kind)?,
            state: SocketState::new(*state)?,
            address: ss_address::parse(local)?,
            processes: ss_users::parse(users.first().copied())?,
        })
    }

    fn parse_local(line: &str) -> Result<ListeningSocket, CollectionError> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [
            kind,
            state,
            _receive_queue,
            _send_queue,
            path,
            _inode,
            _peer_path,
            _peer_inode,
            users @ ..,
        ] = fields.as_slice()
        else {
            return Err(malformed(line, LOCAL_FIELDS, fields.len()));
        };

        Ok(ListeningSocket {
            kind: SocketKind::new(*kind)?,
            state: SocketState::new(*state)?,
            address: SocketAddress::Local {
                path: SocketPath::new(*path)?,
            },
            processes: ss_users::parse(users.first().copied())?,
        })
    }
}

/// The lines of one `ss` run that carry a socket.
fn rows(output: &str) -> impl Iterator<Item = &str> {
    output.lines().filter(|line| !line.trim().is_empty())
}

/// A row that is not the shape this grammar promises.
///
/// The expected count is quoted alongside what arrived, because the two counts are what
/// distinguishes a truncated row from a row of the other family that reached the wrong
/// parser.
fn malformed(line: &str, expected: usize, found: usize) -> CollectionError {
    CollectionError::new(format!(
        "expected at least {} fields in an `{PROGRAM}` row, got {found}: {line:?}",
        expected - 1
    ))
}
