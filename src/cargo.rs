// Refs:
// - https://doc.rust-lang.org/nightly/cargo/index.html

use std::{env, path::PathBuf};

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{config::Config, context::Context, env::Env, process::ProcessBuilder};

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
    pub(crate) fn new(env: &Env, manifest_path: Option<&Utf8Path>) -> Result<Self> {
        let current_manifest = package_root(env, manifest_path.as_deref())?;
        let metadata = metadata(env, &current_manifest)?;

        let cargo = Cargo::new(env, &metadata.workspace_root)?;

        let config = Config::new(&cargo, &metadata.workspace_root)?;

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
        let mut cmd = if self.cargo.nightly {
            let mut rustc = self.cargo.path.clone();
            rustc.pop(); // cargo
            rustc.push(format!("rustc{}", env::consts::EXE_SUFFIX));
            cmd!(rustc)
        } else {
            cmd!("rustup", "run", "nightly", "rustc")
        };
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
fn metadata(env: &Env, manifest_path: &Utf8Path) -> Result<cargo_metadata::Metadata> {
    let mut cmd =
        cmd!(env.cargo(), "metadata", "--format-version", "1", "--manifest-path", manifest_path);
    serde_json::from_str(&cmd.read()?)
        .with_context(|| format!("failed to parse output from {}", cmd))
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
pub(crate) fn test_args(cx: &Context, cmd: &mut ProcessBuilder) {
    let mut has_target_selection_options = false;
    if cx.args.lib {
        has_target_selection_options = true;
        cmd.arg("--lib");
    }
    for name in &cx.args.bin {
        has_target_selection_options = true;
        cmd.arg("--bin");
        cmd.arg(name);
    }
    if cx.args.bins {
        has_target_selection_options = true;
        cmd.arg("--bins");
    }
    for name in &cx.args.example {
        has_target_selection_options = true;
        cmd.arg("--example");
        cmd.arg(name);
    }
    if cx.args.examples {
        has_target_selection_options = true;
        cmd.arg("--examples");
    }
    for name in &cx.args.test {
        has_target_selection_options = true;
        cmd.arg("--test");
        cmd.arg(name);
    }
    if cx.args.tests {
        has_target_selection_options = true;
        cmd.arg("--tests");
    }
    for name in &cx.args.bench {
        has_target_selection_options = true;
        cmd.arg("--bench");
        cmd.arg(name);
    }
    if cx.args.benches {
        has_target_selection_options = true;
        cmd.arg("--benches");
    }
    if cx.args.all_targets {
        has_target_selection_options = true;
        cmd.arg("--all-targets");
    }
    if cx.args.doc {
        has_target_selection_options = true;
        cmd.arg("--doc");
    }

    if !has_target_selection_options && !cx.args.doctests {
        cmd.arg("--tests");
    }

    if cx.args.quiet {
        cmd.arg("--quiet");
    }
    if cx.args.no_fail_fast {
        cmd.arg("--no-fail-fast");
    }
    for package in &cx.args.package {
        cmd.arg("--package");
        cmd.arg(package);
    }
    if cx.args.workspace {
        cmd.arg("--workspace");
    }
    for exclude in &cx.args.exclude {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    if let Some(jobs) = cx.args.jobs {
        cmd.arg("--jobs");
        cmd.arg(jobs.to_string());
    }
    if cx.args.release {
        cmd.arg("--release");
    }
    if let Some(profile) = &cx.args.profile {
        cmd.arg("--profile");
        cmd.arg(profile);
    }
    for features in &cx.args.features {
        cmd.arg("--features");
        cmd.arg(features);
    }
    if cx.args.all_features {
        cmd.arg("--all-features");
    }
    if cx.args.no_default_features {
        cmd.arg("--no-default-features");
    }
    if let Some(target) = &cx.args.target {
        cmd.arg("--target");
        cmd.arg(target);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    if let Some(color) = cx.args.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }
    if cx.args.frozen {
        cmd.arg("--frozen");
    }
    if cx.args.locked {
        cmd.arg("--locked");
    }
    if cx.args.offline {
        cmd.arg("--offline");
    }

    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }

    for unstable_flag in &cx.args.unstable_flags {
        cmd.arg("-Z");
        cmd.arg(unstable_flag);
    }

    if !cx.args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&cx.args.args);
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
pub(crate) fn clean_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.args.quiet {
        cmd.arg("--quiet");
    }
    if cx.args.release {
        cmd.arg("--release");
    }
    if let Some(profile) = &cx.args.profile {
        cmd.arg("--profile");
        cmd.arg(profile);
    }
    if let Some(target) = &cx.args.target {
        cmd.arg("--target");
        cmd.arg(target);
    }
    if let Some(color) = cx.args.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }
    if cx.args.frozen {
        cmd.arg("--frozen");
    }
    if cx.args.locked {
        cmd.arg("--locked");
    }
    if cx.args.offline {
        cmd.arg("--offline");
    }

    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }
}
