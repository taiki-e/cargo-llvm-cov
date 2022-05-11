use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{bail, format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    cli::{Args, ManifestOptions, RunOptions},
    config::Config,
    context::Context,
    env,
    process::ProcessBuilder,
};

pub(crate) struct Workspace {
    pub(crate) name: String,
    pub(crate) config: Config,
    pub(crate) metadata: cargo_metadata::Metadata,
    pub(crate) current_manifest: Utf8PathBuf,

    pub(crate) target_dir: Utf8PathBuf,
    pub(crate) output_dir: Utf8PathBuf,
    pub(crate) doctests_dir: Utf8PathBuf,
    pub(crate) profdata_file: Utf8PathBuf,

    cargo: PathBuf,
    rustc: PathBuf,
    pub(crate) nightly: bool,
    /// Whether `-C instrument-coverage` is available.
    pub(crate) stable_coverage: bool,
}

impl Workspace {
    pub(crate) fn new(
        options: &ManifestOptions,
        target: Option<&str>,
        doctests: bool,
        show_env: bool,
    ) -> Result<Self> {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let rustc = rustc_path(&cargo);
        let (nightly, ref host) = rustc_version(&rustc)?;

        if doctests && !nightly {
            bail!("--doctests flag requires nightly toolchain; consider using `cargo +nightly llvm-cov`")
        }
        let stable_coverage = cmd!(&rustc, "-C", "help").read()?.contains("instrument-coverage");
        if !stable_coverage && !nightly {
            bail!(
                "cargo-llvm-cov requires rustc 1.60+; consider updating toolchain (`rustup update`)
                 or using nightly toolchain (`cargo +nightly llvm-cov`)"
            );
        }

        // Metadata and config
        let current_manifest = package_root(&cargo, options.manifest_path.as_deref())?;
        let metadata = metadata(&cargo, &current_manifest, options)?;
        let config = Config::new(&cargo, target, Some(host))?;

        let target_dir = if let Some(path) = env::var("CARGO_LLVM_COV_TARGET_DIR")? {
            path.into()
        } else if show_env {
            metadata.target_directory.clone()
        } else {
            // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
            // use a subdirectory of the target directory as the actual target directory.
            metadata.target_directory.join("llvm-cov-target")
        };
        let output_dir = metadata.target_directory.join("llvm-cov");
        let doctests_dir = target_dir.join("doctestbins");

        let name = metadata.workspace_root.file_name().unwrap().to_owned();
        let profdata_file = target_dir.join(format!("{}.profdata", name));

        Ok(Self {
            name,
            config,
            metadata,
            current_manifest,
            target_dir,
            output_dir,
            doctests_dir,
            profdata_file,
            cargo: cargo.into(),
            rustc,
            nightly,
            stable_coverage,
        })
    }

    pub(crate) fn cargo(&self, verbose: u8) -> ProcessBuilder {
        let mut cmd = cmd!(&self.cargo);
        // cargo displays env vars only with -vv.
        if verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn rustc(&self) -> ProcessBuilder {
        cmd!(&self.rustc)
    }

    // https://doc.rust-lang.org/nightly/rustc/command-line-arguments.html#--print-print-compiler-information
    pub(crate) fn rustc_print(&self, kind: &str) -> Result<String> {
        Ok(self
            .rustc()
            .args(["--print", kind])
            .read()
            .with_context(|| format!("failed to get {}", kind))?
            .trim()
            .into())
    }
}

fn rustc_path(cargo: impl AsRef<Path>) -> PathBuf {
    // When toolchain override shorthand (`+toolchain`) is used, `rustc` in
    // PATH and `CARGO` environment variable may be different toolchains.
    // When Rust was installed using rustup, the same toolchain's rustc
    // binary is in the same directory as the cargo binary, so we use it.
    let mut rustc = cargo.as_ref().to_owned();
    rustc.pop(); // cargo
    rustc.push(format!("rustc{}", env::consts::EXE_SUFFIX));
    if rustc.exists() {
        rustc
    } else {
        "rustc".into()
    }
}

fn rustc_version(rustc: &Path) -> Result<(bool, String)> {
    let mut cmd = cmd!(rustc, "--version", "--verbose");
    let verbose_version = cmd.read()?;
    let version =
        verbose_version.lines().find_map(|line| line.strip_prefix("release: ")).ok_or_else(
            || format_err!("unexpected version output from `{}`: {}", cmd, verbose_version),
        )?;
    let (_version, channel) = version.split_once('-').unwrap_or_default();
    let nightly = channel == "nightly" || version == "dev";
    let host = verbose_version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| {
            format_err!("unexpected version output from `{}`: {}", cmd, verbose_version)
        })?
        .to_owned();
    Ok((nightly, host))
}

fn package_root(cargo: &OsStr, manifest_path: Option<&Utf8Path>) -> Result<Utf8PathBuf> {
    let package_root = if let Some(manifest_path) = manifest_path {
        manifest_path.to_owned()
    } else {
        locate_project(cargo)?.into()
    };
    Ok(package_root)
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-locate-project.html
fn locate_project(cargo: &OsStr) -> Result<String> {
    cmd!(cargo, "locate-project", "--message-format", "plain").read()
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-metadata.html
fn metadata(
    cargo: &OsStr,
    manifest_path: &Utf8Path,
    options: &ManifestOptions,
) -> Result<cargo_metadata::Metadata> {
    let mut cmd = cmd!(cargo, "metadata", "--format-version=1", "--manifest-path", manifest_path);
    options.cargo_args(&mut cmd);
    serde_json::from_str(&cmd.read()?)
        .with_context(|| format!("failed to parse output from {}", cmd))
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
pub(crate) fn test_args(cx: &Context, args: &Args, cmd: &mut ProcessBuilder) {
    let mut has_target_selection_options = false;
    if args.lib {
        has_target_selection_options = true;
        cmd.arg("--lib");
    }
    for name in &args.bin {
        has_target_selection_options = true;
        cmd.arg("--bin");
        cmd.arg(name);
    }
    if args.bins {
        has_target_selection_options = true;
        cmd.arg("--bins");
    }
    for name in &args.example {
        has_target_selection_options = true;
        cmd.arg("--example");
        cmd.arg(name);
    }
    if args.examples {
        has_target_selection_options = true;
        cmd.arg("--examples");
    }
    for name in &args.test {
        has_target_selection_options = true;
        cmd.arg("--test");
        cmd.arg(name);
    }
    if args.tests {
        has_target_selection_options = true;
        cmd.arg("--tests");
    }
    for name in &args.bench {
        has_target_selection_options = true;
        cmd.arg("--bench");
        cmd.arg(name);
    }
    if args.benches {
        has_target_selection_options = true;
        cmd.arg("--benches");
    }
    if args.all_targets {
        has_target_selection_options = true;
        cmd.arg("--all-targets");
    }
    if args.doc {
        has_target_selection_options = true;
        cmd.arg("--doc");
    }

    if !has_target_selection_options && !cx.doctests {
        cmd.arg("--tests");
    }

    if args.quiet {
        cmd.arg("--quiet");
    }
    if args.no_fail_fast {
        cmd.arg("--no-fail-fast");
    }
    for package in &args.package {
        cmd.arg("--package");
        cmd.arg(package);
    }
    if args.workspace {
        cmd.arg("--workspace");
    }
    for exclude in &args.exclude {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    for exclude in &args.exclude_from_test {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(&cx.ws.target_dir);

    cx.build.cargo_args(cmd);
    cx.manifest.cargo_args(cmd);

    for unstable_flag in &args.unstable_flags {
        cmd.arg("-Z");
        cmd.arg(unstable_flag);
    }

    if !args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&args.args);
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-run.html
pub(crate) fn run_args(cx: &Context, args: &RunOptions, cmd: &mut ProcessBuilder) {
    for name in &args.bin {
        cmd.arg("--bin");
        cmd.arg(name);
    }
    for name in &args.example {
        cmd.arg("--example");
        cmd.arg(name);
    }

    if args.quiet {
        cmd.arg("--quiet");
    }
    if let Some(package) = &args.package {
        cmd.arg("--package");
        cmd.arg(package);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(&cx.ws.target_dir);

    cx.build.cargo_args(cmd);
    cx.manifest.cargo_args(cmd);

    for unstable_flag in &args.unstable_flags {
        cmd.arg("-Z");
        cmd.arg(unstable_flag);
    }

    if !args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&args.args);
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
pub(crate) fn clean_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.build.release {
        cmd.arg("--release");
    }
    if let Some(profile) = &cx.build.profile {
        cmd.arg("--profile");
        cmd.arg(profile);
    }
    if let Some(target) = &cx.build.target {
        cmd.arg("--target");
        cmd.arg(target);
    }
    if let Some(color) = cx.build.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(&cx.ws.target_dir);

    cx.manifest.cargo_args(cmd);

    // If `-vv` is passed, propagate `-v` to cargo.
    if cx.build.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.build.verbose as usize - 1)));
    }
}
