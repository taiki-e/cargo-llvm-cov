#![allow(dead_code, unused_imports)]

use std::{io, path::Path};

pub(crate) use fs_err::*;

pub(crate) fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::create_dir(path) {
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        res => res,
    }
}

pub(crate) fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_file(path) {
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

pub(crate) fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_dir(path) {
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    match fs_err::remove_dir_all(path) {
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}
