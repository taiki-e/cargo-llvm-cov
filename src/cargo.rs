use std::ffi::OsString;

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{context::Context, env::Env, process::ProcessBuilder};

#[derive(Debug)]
pub(crate) struct Cargo {
    path: OsString,
    pub(crate) nightly: bool,
}

impl Cargo {
    pub(crate) fn new(env: &Env, workspace_root: &Utf8Path) -> Result<Self> {
        let path = env.cargo();
        let version = cmd!(path, "--version").dir(workspace_root).read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");

        Ok(Self { path: path.into(), nightly })
    }

    pub(crate) fn nightly_process(&self) -> ProcessBuilder {
        if self.nightly {
            cmd!(&self.path)
        } else {
            cmd!("cargo", "+nightly")
        }
    }

    // https://doc.rust-lang.org/nightly/rustc/command-line-arguments.html#--print-print-compiler-information
    pub(crate) fn rustc_print(&self, kind: &str) -> Result<String> {
        Ok(self
            .nightly_process()
            .args(&["-Z", "unstable-options", "rustc", "--print", kind])
            .read()
            .with_context(|| format!("failed to find {}", kind))?
            .trim()
            .into())
    }
}

pub(crate) fn package_root(manifest_path: Option<&Utf8Path>) -> Result<Utf8PathBuf> {
    let package_root = if let Some(manifest_path) = manifest_path {
        manifest_path.to_owned()
    } else {
        locate_project()?.into()
    };
    Ok(package_root)
}

fn locate_project() -> Result<String> {
    cmd!("cargo", "locate-project", "--message-format", "plain").read()
}

pub(crate) fn metadata(manifest_path: &Utf8Path) -> Result<cargo_metadata::Metadata> {
    Ok(cargo_metadata::MetadataCommand::new().manifest_path(manifest_path).exec()?)
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
