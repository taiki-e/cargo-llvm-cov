// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html

use std::{borrow::Cow, collections::BTreeMap, ffi::OsStr};

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::{env, term::Coloring};

// Note: We don't need to get configuration values like net.offline here,
// because those are configuration that need to be applied only to cargo,
// and such configuration will be handled properly by cargo itself.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) build: Build,
    #[serde(default)]
    target: BTreeMap<String, Target>,
    #[serde(default)]
    pub(crate) doc: Doc,
    #[serde(default)]
    term: Term,
}

impl Config {
    pub(crate) fn new(cargo: &OsStr, target: Option<&str>, host: Option<&str>) -> Result<Self> {
        // Use unstable cargo-config because there is no other good way.
        // However, it is unstable and can break, so allow errors.
        // https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#cargo-config
        // https://github.com/rust-lang/cargo/issues/9301
        // This is the same as what the rust-analyzer does.
        // https://github.com/rust-lang/rust-analyzer/blob/5c88d9344c5b32988bfbfc090f50aba5de1db062/crates/project-model/src/cargo_workspace.rs#L488
        let mut cargo = cmd!(cargo, "-Z", "unstable-options", "config", "get", "--format", "json");
        cargo.env("RUSTC_BOOTSTRAP", "1");
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

        // The following priorities are not documented, but at as of cargo
        // 1.63.0-nightly (2022-05-31), `RUSTC*` are preferred over `CARGO_BUILD_RUSTC*`.
        // 1. RUSTC
        // 2. build.rustc (CARGO_BUILD_RUSTC)
        if let Some(rustc) = env::var("RUSTC")? {
            self.build.rustc = Some(rustc);
        } else if let Some(rustc) = env::var("CARGO_BUILD_RUSTC")? {
            self.build.rustc = Some(rustc);
        }
        // 1. RUSTC_WRAPPER
        // 2. build.rustc-wrapper (CARGO_BUILD_RUSTC_WRAPPER)
        if let Some(rustc_wrapper) = env::var("RUSTC_WRAPPER")? {
            self.build.rustc_wrapper = Some(rustc_wrapper);
        } else if let Some(rustc_wrapper) = env::var("CARGO_BUILD_RUSTC_WRAPPER")? {
            self.build.rustc_wrapper = Some(rustc_wrapper);
        }
        // 1. RUSTC_WORKSPACE_WRAPPER
        // 2. build.rustc-workspace-wrapper (CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER)
        if let Some(rustc_workspace_wrapper) = env::var("RUSTC_WORKSPACE_WRAPPER")? {
            self.build.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
        } else if let Some(rustc_workspace_wrapper) =
            env::var("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")?
        {
            self.build.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
        }

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
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        if let Some(rustflags) = env::var("RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        } else if let Some(target) = target {
            let mut target_rustflags: Option<String> = None;
            for (target_cfg, target_config) in &self.target {
                if let Ok(Some(true)) = target_spec::eval(target_cfg, target) {
                    if let Some(rustflags) = target_config
                        .rustflags
                        .as_ref()
                        .map(StringOrArray::to_string)
                        // cargo ignore empty rustflags field
                        .filter(|s| !s.is_empty())
                    {
                        let target_rustflags = target_rustflags.get_or_insert_with(String::new);
                        if !target_rustflags.is_empty() {
                            target_rustflags.push(' ');
                        }
                        target_rustflags.push_str(&rustflags);
                    }
                }
            }
            if let Some(rustflags) = env::var(&format!(
                "CARGO_TARGET_{}_RUSTFLAGS",
                target.to_uppercase().replace(['-', '.'], "_")
            ))? {
                let target_rustflags = target_rustflags.get_or_insert_with(String::new);
                if !target_rustflags.is_empty() {
                    target_rustflags.push(' ');
                }
                target_rustflags.push_str(&rustflags);
            }
            if let Some(rustflags) = target_rustflags {
                self.build.rustflags = Some(StringOrArray::String(rustflags));
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
        // https://github.com/rust-lang/cargo/blob/0.62.0/src/cargo/ops/cargo_doc.rs#L52-L53
        if self.doc.browser.is_none() {
            if let Some(browser) = env::var("BROWSER")? {
                self.doc.browser = Some(StringOrArray::String(browser));
            }
        }

        if let Some(verbose) = env::var("CARGO_TERM_VERBOSE")? {
            self.term.verbose = Some(verbose.parse()?);
        }
        if let Some(color) = env::var("CARGO_TERM_COLOR")? {
            self.term.color = Some(color.parse()?);
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
            *verbose = u8::from(self.term.verbose.unwrap_or(false));
        }
        if color.is_none() {
            *color = self.term.color;
        }
    }

    pub(crate) fn rustflags(&self) -> Option<Cow<'_, str>> {
        // Refer only build.rustflags because Self::apply_env update build.rustflags
        // based on target.<..>.rustflags.
        self.build.rustflags.as_ref().map(StringOrArray::to_string)
    }

    pub(crate) fn rustdocflags(&self) -> Option<Cow<'_, str>> {
        self.build.rustdocflags.as_ref().map(StringOrArray::to_string)
    }
}

// https://doc.rust-lang.org/nightly/cargo/reference/config.html#build
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Build {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
    pub(crate) rustc: Option<String>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-wrapper
    pub(crate) rustc_wrapper: Option<String>,
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-workspace-wrapper
    pub(crate) rustc_workspace_wrapper: Option<String>,
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

    pub(crate) fn to_string(&self) -> Cow<'_, str> {
        match self {
            Self::String(s) => Cow::Borrowed(s),
            Self::Array(v) => Cow::Owned(v.join(" ")),
        }
    }
}
