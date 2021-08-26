use std::ffi::{OsStr, OsString};

use anyhow::{format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{env::Env, process::ProcessBuilder};

#[derive(Debug)]
pub(crate) struct Rustc {
    path: OsString,
    nightly: bool,
    pub(crate) verbose_version: String,
    pub(crate) host: String,
}

impl Rustc {
    pub(crate) fn new(env: &Env, workspace_root: &Utf8Path) -> Result<Self> {
        let path = env.rustc.as_deref().unwrap_or_else(|| OsStr::new("rustc"));
        let version = cmd!(path, "--version").dir(workspace_root).read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");
        let mut rustc = Self {
            path: path.into(),
            nightly,
            verbose_version: String::new(),
            host: String::new(),
        };

        let mut cmd = rustc.nightly_process();
        cmd.args(&["--version", "--verbose"]);
        rustc.verbose_version = cmd.read()?;
        rustc.host = rustc
            .verbose_version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| {
                format_err!("unexpected version output from `{}`: {}", cmd, rustc.verbose_version)
            })?
            .to_owned();

        Ok(rustc)
    }

    fn nightly_process(&self) -> ProcessBuilder {
        if self.nightly {
            cmd!(&self.path)
        } else {
            cmd!("rustup", "run", "nightly", "rustc")
        }
    }
}

pub(crate) fn sysroot(rustc: &Rustc) -> Result<Utf8PathBuf> {
    Ok(rustc
        .nightly_process()
        .args(&["--print", "sysroot"])
        .read()
        .context("failed to find sysroot")?
        .trim()
        .into())
}
