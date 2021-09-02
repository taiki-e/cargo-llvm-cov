use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::Result;

#[derive(Debug)]
pub(crate) struct Env {
    /// `CARGO_LLVM_COV_FLAGS` environment variable to pass additional flags
    /// to llvm-cov. (value: space-separated list)
    pub(crate) cargo_llvm_cov_flags: Option<String>,
    /// `CARGO_LLVM_PROFDATA_FLAGS` environment variable to pass additional flags
    /// to llvm-profdata. (value: space-separated list)
    pub(crate) cargo_llvm_profdata_flags: Option<String>,

    // Environment variables Cargo sets for 3rd party subcommands
    // https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-3rd-party-subcommands
    /// `CARGO` environment variable.
    pub(crate) cargo: Option<OsString>,

    pub(crate) current_exe: PathBuf,
}

impl Env {
    pub(crate) fn new() -> Result<Self> {
        let cargo_llvm_cov_flags = var("CARGO_LLVM_COV_FLAGS")?;
        let cargo_llvm_profdata_flags = var("CARGO_LLVM_PROFDATA_FLAGS")?;
        env::remove_var("LLVM_COV_FLAGS");
        env::remove_var("LLVM_PROFDATA_FLAGS");
        env::set_var("CARGO_INCREMENTAL", "0");

        Ok(Self {
            cargo_llvm_cov_flags,
            cargo_llvm_profdata_flags,
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

    pub(crate) fn cargo(&self) -> &OsStr {
        self.cargo.as_deref().unwrap_or_else(|| OsStr::new("cargo"))
    }
}

pub(crate) fn var(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
