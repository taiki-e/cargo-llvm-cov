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

pub(crate) fn config(cargo: &Cargo, workspace_root: &Utf8Path) -> Result<Config> {
    let s = cargo
        .process()
        .args(&["-Z", "unstable-options", "config", "get", "--format", "json"])
        .env("RUSTC_BOOTSTRAP", "1")
        .dir(workspace_root)
        .stderr_capture()
        .read()?;
    let mut config: Config = serde_json::from_str(&s)?;
    config.apply_env()?;
    Ok(config)
}

pub(crate) fn locate_project() -> Result<String> {
    cmd!("cargo", "locate-project", "--message-format", "plain").read()
}

pub(crate) fn append_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if !cx.doctests {
        cmd.arg("--tests");
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
    if cx.release {
        cmd.arg("--release");
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
// - https://github.com/rust-lang/cargo/issues/9301

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    build: Build,
    #[serde(default)]
    term: Term,
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

    pub(crate) fn merge_to(self, args: &mut Args, env: &mut Env) {
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
        if args.target.is_none() {
            args.target = self.build.target;
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
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
    target: Option<String>,
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
