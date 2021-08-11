use std::{io, path::Path};

pub(crate) use fs_err::{create_dir_all, read_dir, read_to_string, write};

/// Removes a file from the filesystem **if exists**.
pub(crate) fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_file(path.as_ref()) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

/// Removes a directory at this path **if exists**.
pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_dir_all(path.as_ref()) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}
