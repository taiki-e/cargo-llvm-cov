// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) use std::env::*;
use std::{env, ffi::OsString};

use anyhow::Result;

pub(crate) fn var(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn var_os(key: &str) -> Option<OsString> {
    env::var_os(key).filter(|v| !v.is_empty())
}
