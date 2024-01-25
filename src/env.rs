// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) use std::env::*;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Result;

pub(crate) fn var(key: &str) -> Result<Option<String>> {
    match std::env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn var_os(key: &str) -> Option<OsString> {
    std::env::var_os(key).filter(|v| !v.is_empty())
}

// Use the home crate only on Windows which std::env::home_dir is not correct.
// https://github.com/rust-lang/cargo/blob/b2e1d3b6235c07221dd0fcac54a7b0c754ef8b11/crates/home/src/lib.rs#L65-L72
#[cfg(windows)]
pub(crate) use home::home_dir;
#[cfg(not(windows))]
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir()
}

// Follow the cargo's behavior.
// https://github.com/rust-lang/cargo/blob/b2e1d3b6235c07221dd0fcac54a7b0c754ef8b11/crates/home/src/lib.rs#L77-L86
// https://github.com/rust-lang/cargo/blob/b2e1d3b6235c07221dd0fcac54a7b0c754ef8b11/crates/home/src/lib.rs#L114-L123
// https://github.com/rust-lang/cargo/blob/b2e1d3b6235c07221dd0fcac54a7b0c754ef8b11/crates/home/src/env.rs#L63-L77
// https://github.com/rust-lang/cargo/blob/b2e1d3b6235c07221dd0fcac54a7b0c754ef8b11/crates/home/src/env.rs#L92-L106
pub(crate) fn cargo_home_with_cwd(cwd: &Path) -> Option<PathBuf> {
    match var_os("CARGO_HOME").map(PathBuf::from) {
        Some(home) => {
            if home.is_absolute() {
                Some(home)
            } else {
                Some(cwd.join(home))
            }
        }
        _ => Some(home_dir()?.join(".cargo")),
    }
}
pub(crate) fn rustup_home_with_cwd(cwd: &Path) -> Option<PathBuf> {
    match var_os("RUSTUP_HOME").map(PathBuf::from) {
        Some(home) => {
            if home.is_absolute() {
                Some(home)
            } else {
                Some(cwd.join(home))
            }
        }
        _ => Some(home_dir()?.join(".rustup")),
    }
}
