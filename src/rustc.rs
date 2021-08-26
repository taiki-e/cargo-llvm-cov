use std::ffi::OsStr;

use anyhow::{format_err, Result};
use camino::Utf8Path;

use crate::env::Env;

#[derive(Debug)]
pub(crate) struct RustcInfo {
    pub(crate) verbose_version: String,
    pub(crate) host: String,
}

impl RustcInfo {
    pub(crate) fn new(env: &Env, workspace_root: &Utf8Path) -> Result<Self> {
        let path = env.rustc.as_deref().unwrap_or_else(|| OsStr::new("rustc"));
        let version = cmd!(path, "--version").dir(workspace_root).read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");

        let mut cmd = if nightly { cmd!(path) } else { cmd!("rustup", "run", "nightly", "rustc") };
        cmd.args(&["--version", "--verbose"]);
        let verbose_version = cmd.read()?;
        let host = verbose_version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| {
                format_err!("unexpected version output from `{}`: {}", cmd, verbose_version)
            })?
            .to_owned();

        Ok(Self { verbose_version, host })
    }
}
