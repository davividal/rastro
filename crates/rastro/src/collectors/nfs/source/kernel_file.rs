//! Reading one value the kernel publishes as a file.

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::Path;

use crate::collectors::nfs::value_objects::KernelValue;

/// The most of a value that is read. The longest measured, `portlist`, is a line per listener.
const MOST_READ: u64 = 64 * 1024;

/// The file's value, bounded, and only where it is a regular file.
///
/// Not [`read_inside`](crate::collectors::inside_root::read_inside), which refuses every file a
/// kernel filesystem serves, and these are all such files. The trees are the kernel's own, so
/// nothing in them is another account's to make a FIFO of; the bound and the type check are
/// there so that a misread path costs one value rather than the run.
pub fn read_value(path: &Path) -> KernelValue {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return KernelValue::Unpublished,
        Err(error) => return unreadable(path, &error.to_string()),
    };
    if !metadata.is_file() {
        return unreadable(path, "not a regular file");
    }

    let mut bytes = Vec::new();
    let read = File::open(path).and_then(|file| file.take(MOST_READ + 1).read_to_end(&mut bytes));
    if let Err(error) = read {
        return unreadable(path, &error.to_string());
    }
    if bytes.len() as u64 > MOST_READ {
        return unreadable(path, &format!("longer than {MOST_READ} bytes"));
    }

    match String::from_utf8(bytes) {
        Ok(text) => KernelValue::reported(text),
        Err(_) => unreadable(path, "not valid UTF-8"),
    }
}

fn unreadable(path: &Path, reason: &str) -> KernelValue {
    KernelValue::Unreadable(format!("could not read {}: {reason}", path.display()))
}
