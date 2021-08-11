use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::Result;

#[derive(Debug)]
pub(crate) struct Env {
    // Environment variables Cargo reads
    // https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-reads
    /// `RUSTFLAGS` environment variable.
    pub(crate) rustflags: Option<OsString>,
    /// `RUSTDOCFLAGS` environment variable.
    pub(crate) rustdocflags: Option<OsString>,
    /// `RUSTC` environment variable.
    pub(crate) rustc: Option<OsString>,

    // Environment variables Cargo sets for 3rd party subcommands
    // https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-3rd-party-subcommands
    /// `CARGO` environment variable.
    pub(crate) cargo: Option<OsString>,

    pub(crate) current_exe: PathBuf,
}

impl Env {
    pub(crate) fn new() -> Result<Self> {
        env::remove_var("LLVM_COV_FLAGS");
        env::remove_var("LLVM_PROFDATA_FLAGS");
        env::set_var("CARGO_INCREMENTAL", "0");

        Ok(Self {
            rustflags: env::var_os("RUSTFLAGS"),
            rustdocflags: env::var_os("RUSTDOCFLAGS"),
            rustc: env::var_os("RUSTC"),
            cargo: env::var_os("CARGO"),
            current_exe: match env::current_exe() {
                Ok(exe) => exe,
                Err(e) => {
                    let exe = format!("cargo-llvm-cov{}", env::consts::EXE_SUFFIX);
                    warn!("failed to get current executable, assuming {} in PATH as current executable: {}", exe, e);
                    exe.into()
                }
            },
        })
    }

    pub(crate) fn rustc(&self) -> &OsStr {
        self.rustc.as_deref().unwrap_or_else(|| OsStr::new("rustc"))
    }

    pub(crate) fn cargo(&self) -> &OsStr {
        self.cargo.as_deref().unwrap_or_else(|| OsStr::new("cargo"))
    }
}

pub(crate) fn ver(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
