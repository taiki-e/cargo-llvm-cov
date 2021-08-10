pub(crate) use std::fs::ReadDir;
use std::{io, path::Path};

use anyhow::{Context as _, Result};

/// Recursively create a directory.
/// This is a wrapper for [`std::fs::create_dir_all`].
#[track_caller]
pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let res = std::fs::create_dir_all(path);
    trace!(track_caller: ?res, ?path, "create_dir_all");
    res.with_context(|| format!("failed to create directory `{}`", path.display()))
}

/// Removes a file from the filesystem **if exists**.
/// This is a wrapper for [`std::fs::remove_file`].
#[track_caller]
pub(crate) fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            trace!(track_caller: res = ?Ok::<_, ()>(e), ?path, "remove_file");
            Ok(())
        }
        res => {
            trace!(track_caller: ?res, ?path, "remove_file");
            res.with_context(|| format!("failed to remove file `{}`", path.display()))
        }
    }
}

/// Removes a directory at this path **if exists**.
/// This is a wrapper for [`std::fs::remove_dir_all`].
#[track_caller]
pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            trace!(track_caller: res = ?Ok::<_, ()>(e), ?path, "remove_dir_all");
            Ok(())
        }
        res => {
            trace!(track_caller: ?res, ?path, "remove_dir_all");
            res.with_context(|| format!("failed to remove directory `{}`", path.display()))
        }
    }
}

/// Write a slice as the entire contents of a file.
/// This is a wrapper for [`std::fs::write`].
#[track_caller]
pub(crate) fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let res = std::fs::write(path, contents.as_ref());
    trace!(track_caller: ?res, ?path, "write");
    res.with_context(|| format!("failed to write to file `{}`", path.display()))
}

/// Read the entire contents of a file into a string.
/// This is a wrapper for [`std::fs::read_to_string`].
#[track_caller]
pub(crate) fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let res = std::fs::read_to_string(path);
    trace!(track_caller: ?res, ?path, "read_to_string");
    res.with_context(|| format!("failed to read from file `{}`", path.display()))
}

/// Returns an iterator over the entries within a directory.
/// This is a wrapper for [`std::fs::read_dir`].
#[track_caller]
pub(crate) fn read_dir(path: impl AsRef<Path>) -> Result<ReadDir> {
    let path = path.as_ref();
    let res = std::fs::read_dir(path);
    trace!(track_caller: ?res, ?path, "read_dir");
    res.with_context(|| format!("failed to read directory `{}`", path.display()))
}
