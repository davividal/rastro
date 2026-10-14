#![allow(dead_code)]

//! The account the tests run as, which a fixture server's files are read with.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rastro::collectors::inside_root::{ProcessRoot, ReadingAccount};

/// The user and group the test process creates files as: what `geteuid` would say, read without
/// the unsafe call.
pub fn runner() -> ReadingAccount {
    let probe = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "account-probe-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::write(&probe, "").expect("a writable scratch file");
    let owner = fs::metadata(&probe).expect("a scratch file");
    let _ = fs::remove_file(&probe);

    ReadingAccount::new(owner.uid(), owner.gid(), Vec::new())
}

/// Whether the tests run as root, which can take on another account's permission.
pub fn running_as_root() -> bool {
    runner().user() == 0
}

/// rastro's own root, read as the account the tests run as: a server whose paths and owner are
/// the test's.
pub fn host_root() -> ProcessRoot {
    ProcessRoot::new("/", runner())
}

/// A `/proc/<pid>/status` naming the account the tests run as, in the lines rastro reads.
pub fn status_of_runner() -> String {
    let account = runner();
    let (user, group) = (account.user(), account.group());

    format!(
        "Name:\tredis-server\nUid:\t{user}\t{user}\t{user}\t{user}\nGid:\t{group}\t{group}\t{group}\t{group}\nGroups:\t\n"
    )
}
