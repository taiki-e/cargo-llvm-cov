use std::env;

use anyhow::{format_err, Result};
use serde::Deserialize;

use crate::{
    cli::{Args, Coloring},
    context::EnvironmentVariables,
};

fn ver(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// =============================================================================
// Cargo manifest
// https://doc.rust-lang.org/nightly/cargo/reference/manifest.html

#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) package: Option<Package>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/manifest.html#the-package-section
#[derive(Debug, Deserialize)]
pub(crate) struct Package {
    pub(crate) name: String,
}

// =============================================================================
// Cargo configuration
// https://doc.rust-lang.org/nightly/cargo/reference/config.html

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    build: Build,
    #[serde(default)]
    term: Term,
}

impl Config {
    // Apply configuration environment variables
    pub(crate) fn apply_env(&mut self) -> Result<()> {
        // Environment variables are prefer over config values.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables
        if let Some(rustc) = ver("CARGO_BUILD_RUSTC")? {
            self.build.rustc = Some(rustc);
        }
        if let Some(rustflags) = ver("CARGO_BUILD_RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        }
        if let Some(rustdocflags) = ver("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.build.rustdocflags = Some(StringOrArray::String(rustdocflags));
        }
        if let Some(verbose) = ver("CARGO_TERM_VERBOSE")? {
            self.term.verbose = Some(verbose.parse()?);
        }
        if let Some(color) = ver("CARGO_TERM_COLOR")? {
            self.term.color =
                Some(clap::ArgEnum::from_str(&color, false).map_err(|e| format_err!("{}", e))?);
        }
        Ok(())
    }

    pub(crate) fn merge_to(self, args: &mut Args, env: &mut EnvironmentVariables) {
        // RUSTFLAGS environment variable is prefer over build.rustflags config value.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        if env.rustflags.is_none() {
            if let Some(rustflags) = self.build.rustflags {
                env.rustflags = Some(rustflags.into_string().into());
            }
        }
        // RUSTDOCFLAGS environment variable is prefer over build.rustdocflags config value.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
        if env.rustdocflags.is_none() {
            if let Some(rustdocflags) = self.build.rustdocflags {
                env.rustdocflags = Some(rustdocflags.into_string().into());
            }
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
        if env.rustc.is_none() {
            if let Some(rustc) = self.build.rustc {
                env.rustc = Some(rustc.into());
            }
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
struct Build {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
    rustc: Option<String>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
    rustflags: Option<StringOrArray>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
    rustdocflags: Option<StringOrArray>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#term
#[derive(Debug, Default, Deserialize)]
struct Term {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose
    verbose: Option<bool>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor
    color: Option<Coloring>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

impl StringOrArray {
    fn into_string(self) -> String {
        match self {
            Self::String(s) => s,
            Self::Array(v) => v.join(" "),
        }
    }
}
