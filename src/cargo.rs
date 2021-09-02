// Refs:
// - https://doc.rust-lang.org/nightly/cargo/index.html

use std::{env, path::PathBuf};

use anyhow::{format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    cli::{Args, ManifestOptions, RunOptions},
    config::Config,
    context::Context,
    env::Env,
    process::ProcessBuilder,
};

pub(crate) struct Cargo {
    path: PathBuf,
    pub(crate) nightly: bool,
}

impl Cargo {
    fn new(env: &Env, workspace_root: &Utf8Path) -> Result<Self> {
        let path = env.cargo();
        let version = cmd!(path, "--version").dir(workspace_root).read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");

        Ok(Self { path: path.into(), nightly })
    }

    pub(crate) fn process(&self) -> ProcessBuilder {
        if self.nightly {
            cmd!(&self.path)
        } else {
            cmd!("cargo", "+nightly")
        }
    }

    fn rustc_process(&self) -> ProcessBuilder {
        if self.nightly {
            let mut rustc = self.path.clone();
            rustc.pop(); // cargo
            rustc.push(format!("rustc{}", env::consts::EXE_SUFFIX));
            cmd!(rustc)
        } else {
            cmd!("rustup", "run", "nightly", "rustc")
        }
    }
}

pub(crate) struct Workspace {
    pub(crate) config: Config,

    pub(crate) target_dir: Utf8PathBuf,
    pub(crate) output_dir: Utf8PathBuf,
    pub(crate) doctests_dir: Utf8PathBuf,
    pub(crate) package_name: String,
    pub(crate) profdata_file: Utf8PathBuf,

    pub(crate) metadata: cargo_metadata::Metadata,
    pub(crate) current_manifest: Utf8PathBuf,

    pub(crate) cargo: Cargo,
}

impl Workspace {
    pub(crate) fn new(env: &Env, options: &ManifestOptions, target: Option<&str>) -> Result<Self> {
        let current_manifest = package_root(env, options.manifest_path.as_deref())?;
        let metadata = metadata(env, &current_manifest, options)?;

        let cargo = Cargo::new(env, &metadata.workspace_root)?;
        let mut rustc = cargo.rustc_process();
        rustc.args(["--version", "--verbose"]);
        let verbose_version = rustc.read()?;
        let host = verbose_version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| {
                format_err!("unexpected version output from `{}`: {}", rustc, verbose_version)
            })?
            .to_owned();

        let config = Config::new(&cargo, &metadata.workspace_root, target, Some(&host))?;

        // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
        // use a subdirectory of the target directory as the actual target directory.
        let target_dir = metadata.target_directory.join("llvm-cov-target");
        let output_dir = metadata.target_directory.join("llvm-cov");
        let doctests_dir = target_dir.join("doctestbins");

        let package_name = metadata.workspace_root.file_stem().unwrap().to_string();
        let profdata_file = target_dir.join(format!("{}.profdata", package_name));

        Ok(Self {
            config,
            target_dir,
            output_dir,
            doctests_dir,
            package_name,
            profdata_file,
            metadata,
            current_manifest,
            cargo,
        })
    }

    pub(crate) fn cargo_process(&self, verbose: u8) -> ProcessBuilder {
        let mut cmd = self.cargo.process();
        cmd.dir(&self.metadata.workspace_root);
        // cargo displays env vars only with -vv.
        if verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn rustc_process(&self) -> ProcessBuilder {
        let mut cmd = self.cargo.rustc_process();
        cmd.dir(&self.metadata.workspace_root);
        cmd
    }

    // https://github.com/rust-lang/cargo/issues/9357
    // https://doc.rust-lang.org/nightly/rustc/command-line-arguments.html#--print-print-compiler-information
    pub(crate) fn rustc_print(&self, kind: &str) -> Result<String> {
        // `cargo rustc --print` is more accurate than `rustc --print` in cargo
        // subcommand's context. However, allow error from `cargo rustc --print`
        // as it is an unstable feature, and fallback to `rustc --print`.
        Ok(match self
            .cargo_process(0)
            .args(["-Z", "unstable-options", "rustc", "--print", kind])
            .read()
        {
            Ok(s) => Ok(s),
            Err(e) => match self.rustc_process().args(["--print", kind]).read() {
                Ok(s) => {
                    warn!("{}", e);
                    Ok(s)
                }
                Err(e) => Err(e),
            },
        }
        .with_context(|| format!("failed to get {}", kind))?
        .trim()
        .into())
    }
}

fn package_root(env: &Env, manifest_path: Option<&Utf8Path>) -> Result<Utf8PathBuf> {
    let package_root = if let Some(manifest_path) = manifest_path {
        manifest_path.to_owned()
    } else {
        locate_project(env)?.into()
    };
    Ok(package_root)
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-locate-project.html
fn locate_project(env: &Env) -> Result<String> {
    cmd!(env.cargo(), "locate-project", "--message-format", "plain").read()
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-metadata.html
fn metadata(
    env: &Env,
    manifest_path: &Utf8Path,
    options: &ManifestOptions,
) -> Result<cargo_metadata::Metadata> {
    let mut cmd =
        cmd!(env.cargo(), "metadata", "--format-version", "1", "--manifest-path", manifest_path);
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

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

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
    if cx.quiet {
        cmd.arg("--quiet");
    }
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

    cx.manifest.cargo_args(cmd);

    if cx.build.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.build.verbose as usize - 1)));
    }
}
