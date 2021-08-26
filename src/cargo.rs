use std::ffi::{OsStr, OsString};

use anyhow::{format_err, Result};
use camino::Utf8Path;
use serde::Deserialize;

use crate::{
    cli::{Args, Coloring},
    context::Context,
    env::{self, Env},
    process::ProcessBuilder,
};

#[derive(Debug)]
pub(crate) struct Cargo {
    path: OsString,
    pub(crate) nightly: bool,
}

impl Cargo {
    pub(crate) fn new(env: &Env, workspace_root: &Utf8Path) -> Result<Self> {
        let mut path = env.cargo();
        let version = cmd!(path, "version").dir(workspace_root).read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");
        if !nightly {
            path = OsStr::new("cargo");
        }

        Ok(Self { path: path.into(), nightly })
    }

    pub(crate) fn process(&self) -> ProcessBuilder {
        let mut cmd = cmd!(&self.path);
        if !self.nightly {
            cmd.arg("+nightly");
        }
        cmd
    }
}

pub(crate) fn locate_project() -> Result<String> {
    cmd!("cargo", "locate-project", "--message-format", "plain").read()
}

pub(crate) fn append_args(cx: &Context, cmd: &mut ProcessBuilder) {
    let mut has_target_selection_options = false;
    if cx.lib {
        has_target_selection_options = true;
        cmd.arg("--lib");
    }
    for name in &cx.bin {
        has_target_selection_options = true;
        cmd.arg("--bin");
        cmd.arg(name);
    }
    if cx.bins {
        has_target_selection_options = true;
        cmd.arg("--bins");
    }
    for name in &cx.example {
        has_target_selection_options = true;
        cmd.arg("--example");
        cmd.arg(name);
    }
    if cx.examples {
        has_target_selection_options = true;
        cmd.arg("--examples");
    }
    for name in &cx.test {
        has_target_selection_options = true;
        cmd.arg("--test");
        cmd.arg(name);
    }
    if cx.tests {
        has_target_selection_options = true;
        cmd.arg("--tests");
    }
    for name in &cx.bench {
        has_target_selection_options = true;
        cmd.arg("--bench");
        cmd.arg(name);
    }
    if cx.benches {
        has_target_selection_options = true;
        cmd.arg("--benches");
    }
    if cx.all_targets {
        has_target_selection_options = true;
        cmd.arg("--all-targets");
    }
    if cx.doc {
        has_target_selection_options = true;
        cmd.arg("--doc");
    }

    if !has_target_selection_options && !cx.doctests {
        cmd.arg("--tests");
    }

    if cx.quiet {
        cmd.arg("--quiet");
    }
    if cx.no_fail_fast {
        cmd.arg("--no-fail-fast");
    }
    for package in &cx.package {
        cmd.arg("--package");
        cmd.arg(package);
    }
    if cx.workspace {
        cmd.arg("--workspace");
    }
    for exclude in &cx.exclude {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    if let Some(jobs) = cx.jobs {
        cmd.arg("--jobs");
        cmd.arg(jobs.to_string());
    }
    if cx.release {
        cmd.arg("--release");
    }
    if let Some(profile) = &cx.profile {
        cmd.arg("--profile");
        cmd.arg(profile);
    }
    for features in &cx.features {
        cmd.arg("--features");
        cmd.arg(features);
    }
    if cx.all_features {
        cmd.arg("--all-features");
    }
    if cx.no_default_features {
        cmd.arg("--no-default-features");
    }
    if let Some(target) = &cx.target {
        cmd.arg("--target");
        cmd.arg(target);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.manifest_path);

    if let Some(color) = cx.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }
    if cx.frozen {
        cmd.arg("--frozen");
    }
    if cx.locked {
        cmd.arg("--locked");
    }
    if cx.offline {
        cmd.arg("--offline");
    }

    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }

    for unstable_flag in &cx.unstable_flags {
        cmd.arg("-Z");
        cmd.arg(unstable_flag);
    }

    if !cx.args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&cx.args.args);
    }
}

// =============================================================================
// Cargo configuration
//
// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html
// - https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#cargo-config
// - https://github.com/rust-lang/cargo/issues/9301

pub(crate) fn config(cargo: &Cargo, workspace_root: &Utf8Path) -> Result<Config> {
    let mut config = match cargo
        .process()
        .args(&["-Z", "unstable-options", "config", "get", "--format", "json"])
        .env("RUSTC_BOOTSTRAP", "1")
        .dir(workspace_root)
        .stderr_capture()
        .read()
    {
        Ok(s) => serde_json::from_str(&s)?,
        Err(e) => {
            // Allow error from cargo-config as it is an unstable feature.
            warn!("{:#}", e);
            Config::default()
        }
    };
    config.apply_env()?;
    Ok(config)
}

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
    // Apply configuration environment variables
    fn apply_env(&mut self) -> Result<()> {
        // Environment variables are prefer over config values.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables
        if let Some(rustc) = env::ver("CARGO_BUILD_RUSTC")? {
            self.build.rustc = Some(rustc);
        }
        if let Some(rustflags) = env::ver("CARGO_BUILD_RUSTFLAGS")? {
            self.build.rustflags = Some(StringOrArray::String(rustflags));
        }
        if let Some(rustdocflags) = env::ver("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.build.rustdocflags = Some(StringOrArray::String(rustdocflags));
        }
        if let Some(target) = env::ver("CARGO_BUILD_TARGET")? {
            self.build.target = Some(target);
        }
        if let Some(verbose) = env::ver("CARGO_TERM_VERBOSE")? {
            self.term.verbose = Some(verbose.parse()?);
        }
        if let Some(color) = env::ver("CARGO_TERM_COLOR")? {
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
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
        if env.rustc.is_none() {
            if let Some(rustc) = &self.build.rustc {
                env.rustc = Some(rustc.into());
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
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
    pub(crate) rustc: Option<String>,
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
