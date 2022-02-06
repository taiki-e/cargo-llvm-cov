// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html

use std::{collections::BTreeMap, ffi::OsStr};

use anyhow::{format_err, Context as _, Result};
use camino::Utf8Path;
use serde::Deserialize;

use crate::{env, process::ProcessBuilder, term::Coloring};

// NOTE: We don't need to get configuration values like net.offline here,
// because those are configuration that need to be applied only to cargo,
// and such configuration will be handled properly by cargo itself.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    build: Build,
    #[serde(default)]
    target: BTreeMap<String, Target>,
    #[serde(default)]
    pub(crate) doc: Doc,
    #[serde(default)]
    term: Term,
}

impl Config {
    pub(crate) fn new(
        mut cargo: ProcessBuilder,
        workspace_root: &Utf8Path,
        target: Option<&str>,
        host: Option<&str>,
    ) -> Result<Self> {
        // Use unstable cargo-config because there is no other good way.
        // However, it is unstable and can break, so allow errors.
        // https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#cargo-config
        // https://github.com/rust-lang/cargo/issues/9301
        cargo
            .args(&["-Z", "unstable-options", "config", "get", "--format", "json"])
            .dir(workspace_root)
            .env("RUSTC_BOOTSTRAP", "1");
        let mut config = match cargo.read() {
            Ok(s) => serde_json::from_str(&s)
                .with_context(|| format!("failed to parse output from {}", cargo))?,
            Err(e) => {
                // Allow error from cargo-config as it is an unstable feature.
                warn!("{:#}", e);
                Self::default()
            }
        };
        config.apply_env(target, host)?;
        Ok(config)
    }

    // Apply configuration environment variables
    fn apply_env(&mut self, target: Option<&str>, host: Option<&str>) -> Result<()> {
        // Environment variables are prefer over config values.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
        // TODO: Handles the case where this is a relative path to the target spec file.
        if let Some(target) = target {
            self.build.target = Some(target.to_owned());
        } else if let Some(target) = env::var("CARGO_BUILD_TARGET")? {
            self.build.target = Some(target);
        }
        let target = self.build.target.as_deref().or(host);

        // 1. RUSTFLAGS
        // 2. target.<triple>.rustflags (CARGO_TARGET_<triple>_RUSTFLAGS) and target.<cfg>.rustflags
        // 3. build.rustflags (CARGO_BUILD_RUSTFLAGS)
        // NOTE: target.<cfg>.rustflags is currently ignored
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        if let Some(rustflags) = env::var("RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        } else if let Some(target) = target {
            if let Some(rustflags) = env::var(&format!(
                "CARGO_TARGET_{}_RUSTFLAGS",
                target.to_uppercase().replace('-', "_")
            ))? {
                self.build.rustflags = Some(StringOrArray::String(rustflags));
            } else if let Some(Target { rustflags: Some(rustflags) }) = self.target.get(target) {
                self.build.rustflags = Some(rustflags.clone());
            } else if let Some(rustflags) = env::var("CARGO_BUILD_RUSTFLAGS")? {
                self.build.rustflags = Some(StringOrArray::String(rustflags));
            }
        } else if let Some(rustflags) = env::var("CARGO_BUILD_RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        }

        // 1. RUSTDOCFLAGS
        // 2. build.rustdocflags (CARGO_BUILD_RUSTDOCFLAGS)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
        if let Some(rustdocflags) = env::var("RUSTDOCFLAGS")? {
            self.build.rustdocflags = Some(StringOrArray::String(rustdocflags));
        } else if let Some(rustdocflags) = env::var("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.build.rustdocflags = Some(StringOrArray::String(rustdocflags));
        }

        // doc.browser config value is prefer over BROWSER environment variable.
        // https://github.com/rust-lang/cargo/blob/0.55.0/src/cargo/ops/cargo_doc.rs#L58-L59
        if self.doc.browser.is_none() {
            if let Some(browser) = env::var("BROWSER")? {
                self.doc.browser = Some(StringOrArray::String(browser));
            }
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

    pub(crate) fn merge_to_args(
        &self,
        target: &mut Option<String>,
        verbose: &mut u8,
        color: &mut Option<Coloring>,
    ) {
        // CLI flags are prefer over config values.
        if target.is_none() {
            *target = self.build.target.clone();
        }
        if *verbose == 0 {
            *verbose = self.term.verbose.unwrap_or(false) as _;
        }
        if color.is_none() {
            *color = self.term.color;
        }
    }

    pub(crate) fn rustflags(&self) -> Option<String> {
        // Refer only build.rustflags because Self::apply_env update build.rustflags
        // based on target.<..>.rustflags.
        self.build.rustflags.as_ref().map(ToString::to_string)
    }

    pub(crate) fn rustdocflags(&self) -> Option<String> {
        self.build.rustdocflags.as_ref().map(ToString::to_string)
    }
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#build
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Build {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
    rustflags: Option<StringOrArray>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
    rustdocflags: Option<StringOrArray>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
    target: Option<String>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#target
#[derive(Debug, Deserialize)]
struct Target {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerustflags
    rustflags: Option<StringOrArray>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#doc
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Doc {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#docbrowser
    pub(crate) browser: Option<StringOrArray>,
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#term
#[derive(Debug, Default, Deserialize)]
struct Term {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose
    verbose: Option<bool>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor
    color: Option<Coloring>,
}

#[derive(Debug, Clone, Deserialize)]
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
