// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html
// - https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#cargo-config
// - https://github.com/rust-lang/cargo/issues/9301

use std::ffi::OsStr;

use anyhow::{format_err, Result};
use camino::Utf8Path;
use serde::Deserialize;

use crate::{
    cargo::Cargo,
    cli::{Args, Coloring},
    env::{self, Env},
};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) build: Build,
    #[serde(default)]
    pub(crate) doc: Doc,
    #[serde(default)]
    pub(crate) term: Term,
}

impl Config {
    pub(crate) fn new(cargo: &Cargo, workspace_root: &Utf8Path) -> Result<Self> {
        let mut config = match cargo
            .nightly_process()
            .args(&["-Z", "unstable-options", "config", "get", "--format", "json"])
            .dir(workspace_root)
            .stderr_capture()
            .read()
        {
            Ok(s) => serde_json::from_str(&s)?,
            Err(e) => {
                // Allow error from cargo-config as it is an unstable feature.
                warn!("{:#}", e);
                Self::default()
            }
        };
        config.apply_env()?;
        Ok(config)
    }

    // Apply configuration environment variables
    fn apply_env(&mut self) -> Result<()> {
        // Environment variables are prefer over config values.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables
        if let Some(rustflags) = env::var("CARGO_BUILD_RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        }
        if let Some(rustdocflags) = env::var("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.build.rustdocflags = Some(StringOrArray::String(rustdocflags));
        }
        if let Some(target) = env::var("CARGO_BUILD_TARGET")? {
            self.build.target = Some(target);
        }
        if let Some(verbose) = env::var("CARGO_TERM_VERBOSE")? {
            self.term.verbose = Some(verbose.parse()?);
        }
        if let Some(color) = env::var("CARGO_TERM_COLOR")? {
            self.term.color =
                Some(clap::ArgEnum::from_str(&color, false).map_err(|e| format_err!("{}", e))?);
        }
        Ok(())
    }

    pub(crate) fn merge_to(&self, args: &mut Args, env: &mut Env) {
        // RUSTFLAGS environment variable is prefer over build.rustflags config value.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        if env.rustflags.is_none() {
            if let Some(rustflags) = &self.build.rustflags {
                env.rustflags = Some(rustflags.to_string().into());
            }
        }
        // RUSTDOCFLAGS environment variable is prefer over build.rustdocflags config value.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
        if env.rustdocflags.is_none() {
            if let Some(rustdocflags) = &self.build.rustdocflags {
                env.rustdocflags = Some(rustdocflags.to_string().into());
            }
        }
        if args.target.is_none() {
            args.target = self.build.target.clone();
        }
        if args.verbose == 0 && self.term.verbose.unwrap_or(false) {
            args.verbose = 1;
        }
        if args.color.is_none() {
            args.color = self.term.color;
        }
    }
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#build
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Build {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
    pub(crate) rustflags: Option<StringOrArray>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
    pub(crate) rustdocflags: Option<StringOrArray>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
    pub(crate) target: Option<String>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#doc
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Doc {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#docbrowser
    pub(crate) browser: Option<StringOrArray>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#term
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Term {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose
    pub(crate) verbose: Option<bool>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor
    pub(crate) color: Option<Coloring>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

impl StringOrArray {
    pub(crate) fn path_and_args(&self) -> Option<(&OsStr, Vec<&str>)> {
        match self {
            Self::String(s) => {
                let mut s = s.split(' ');
                let path = s.next()?;
                Some((OsStr::new(path), s.collect()))
            }
            Self::Array(v) => {
                let path = v.get(0)?;
                Some((OsStr::new(path), v.iter().skip(1).map(String::as_str).collect()))
            }
        }
    }
}

impl ToString for StringOrArray {
    fn to_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Array(v) => v.join(" "),
        }
    }
}
