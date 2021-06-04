#![allow(dead_code, unused_imports)]

use std::{io, path::Path};

pub(crate) use fs_err::*;

/// Creates a new, empty directory **if not exists**.
/// This is a wrapper for [`std::fs::create_dir`].
pub(crate) fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::create_dir(path) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        res => res,
    }
}

/// Recursively create a directory **if not exists**.
/// This is a wrapper for [`std::fs::create_dir_all`].
pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::create_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        res => res,
    }
}

/// Removes a file from the filesystem **if exists**.
/// This is a wrapper for [`std::fs::remove_file`].
pub(crate) fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

/// Removes an empty directory **if exists**.
/// This is a wrapper for [`std::fs::remove_dir`].
pub(crate) fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_dir(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

/// Removes a directory at this path **if exists**.
/// This is a wrapper for [`std::fs::remove_dir_all`].
pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}
