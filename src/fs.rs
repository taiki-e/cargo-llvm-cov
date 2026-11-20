// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) use std::fs::Metadata;
use std::{ffi::OsStr, io, path::Path};

use anyhow::{Context as _, Result};

/// A wrapper for [`std::fs::File::open`].
#[allow(clippy::disallowed_types)] // we are crate::fs
pub(crate) fn open(path: impl AsRef<Path>) -> Result<std::fs::File> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = std::fs::File::open(path);
    res.with_context(|| format!("failed to open file `{}`", path.display()))
}

/// A wrapper for [`std::fs::write`].
pub(crate) fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = std::fs::write(path, contents.as_ref());
    res.with_context(|| format!("failed to write to file `{}`", path.display()))
}

/// A wrapper for [`std::fs::create_dir_all`].
pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = std::fs::create_dir_all(path);
    res.with_context(|| format!("failed to create directory `{}`", path.display()))
}

/// A wrapper for [`std::fs::read_dir`].
#[allow(clippy::disallowed_types)] // we are crate::fs
pub(crate) fn read_dir(path: impl AsRef<Path>) -> Result<std::fs::ReadDir> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = std::fs::read_dir(path);
    res.with_context(|| format!("failed to read directory `{}`", path.display()))
}

/// Removes a file from the filesystem **if exists**. (Similar to `rm -f`)
pub(crate) fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = match std::fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    };
    res.with_context(|| format!("failed to remove file `{}`", path.display()))
}

/// Removes a directory at this path **if exists**. (Similar to `rm -rf`)
pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    #[allow(clippy::disallowed_methods)] // we are crate::fs
    let res = match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    };
    res.with_context(|| format!("failed to remove directory `{}`", path.display()))
}

/// `a.tar.gz` -> `a` (Note: normal file_stem returns `a.tar`)
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
