pub(crate) use std::fs::Metadata;
use std::{ffi::OsStr, io, path::Path};

pub(crate) use fs_err::{create_dir_all, read_dir, read_to_string, symlink_metadata, write};

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

pub(crate) fn file_stem_recursive(path: &Path) -> Option<&OsStr> {
    let mut file_name = path.file_name()?;
    while let Some(stem) = Path::new(file_name).file_stem() {
        if file_name == stem {
            break;
        }
        file_name = stem;
    }
    Some(file_name)
}
