// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ffi::OsStr;

use anyhow::{Context as _, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_config2::Config;

use crate::{
    cli::Subcommand,
    context::Context,
    env,
    metadata::{Metadata, PackageId},
    process::ProcessBuilder,
};

pub(crate) struct Workspace {
    pub(crate) name: String,
    pub(crate) config: Config,
    pub(crate) metadata: Metadata,
    pub(crate) current_manifest: Utf8PathBuf,
    pub(crate) current_package: Option<PackageId>,

    pub(crate) target_dir: Utf8PathBuf,
    pub(crate) build_dir: Option<Utf8PathBuf>,
    pub(crate) default_output_dir: Utf8PathBuf,
    pub(crate) doctests_dir: Utf8PathBuf,
    pub(crate) profdata_file: Utf8PathBuf,

    rustc: ProcessBuilder,
    pub(crate) target_for_config: cargo_config2::TargetTriple,
    pub(crate) target_is_windows: bool,
    pub(crate) rustc_version: cargo_config2::RustcVersion,
}

impl Workspace {
    pub(crate) fn new(
        manifest_path: Option<&Utf8Path>,
        target: Option<&str>,
        show_env: bool,
    ) -> Result<Self> {
        // Metadata and config
        let config = Config::load()?;
        let current_manifest = locate_project(config.cargo(), manifest_path)?;
        let metadata = Metadata::new(current_manifest.as_std_path(), config.cargo())?;
        let mut target_for_config = config.build_target_for_config(target)?;
        if target_for_config.len() != 1 {
            bail!(
                "cargo-llvm-cov doesn't currently supports multi-target builds: {target_for_config:?}"
            );
        }
        let target_for_config = target_for_config.pop().unwrap();
        let target_is_windows = target_for_config.triple().contains("-windows");
        let rustc = ProcessBuilder::from(config.rustc().clone());
        let mut rustc_version = config.rustc_version()?;
        rustc_version.nightly =
            rustc_version.nightly || env::var_os("RUSTC_BOOTSTRAP").unwrap_or_default() == "1";

        let mut current_package = None;
        for &id in &metadata.workspace_members {
            let manifest_path = &*metadata[id].manifest_path;
            // no need to use same_file as cargo-metadata and cargo-locate-project
            // as they return absolute paths resolved in the same way.
            if manifest_path == current_manifest {
                current_package = Some(id);
            }
        }

        let (target_dir, build_dir) = if let Some(mut target_dir) =
            env::var("CARGO_LLVM_COV_TARGET_DIR")?.map(Utf8PathBuf::from)
        {
            let mut base: Utf8PathBuf = env::current_dir()?.try_into()?;
            target_dir = base.join(target_dir);
            let build_dir = if let Some(build_dir) =
                env::var("CARGO_LLVM_COV_BUILD_DIR")?.map(Utf8PathBuf::from)
            {
                base.push(build_dir);
                base
            } else {
                target_dir.clone()
            };
            (target_dir, build_dir)
        } else if show_env {
            (metadata.target_directory.clone(), metadata.build_directory().to_owned())
        } else {
            // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
            // use a subdirectory of the target directory as the actual target directory.
            (
                metadata.target_directory.join("llvm-cov-target"),
                metadata.build_directory().join("llvm-cov-target"),
            )
        };
        // The scope of --target-dir's effect depends on whether build-dir is specified in the config.
        let build_dir = config.build.build_dir.as_ref().and(Some(build_dir));
        let default_output_dir = metadata.target_directory.join("llvm-cov");
        let doctests_dir = target_dir.join("doctestbins");

        let name = metadata.workspace_root.file_name().unwrap_or("default").to_owned();
        let profdata_file = target_dir.join(format!("{name}.profdata"));

        Ok(Self {
            name,
            config,
            metadata,
            current_manifest,
            current_package,
            target_dir,
            build_dir,
            default_output_dir,
            doctests_dir,
            profdata_file,
            rustc,
            target_for_config,
            target_is_windows,
            rustc_version,
        })
    }

    pub(crate) fn cargo(&self, verbose: u8) -> ProcessBuilder {
        let mut cmd = cmd!(self.config.cargo());
        // cargo displays env vars only with -vv.
        if verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn rustc(&self) -> ProcessBuilder {
        self.rustc.clone()
    }

    // https://doc.rust-lang.org/nightly/rustc/command-line-arguments.html#--print-print-compiler-information
    pub(crate) fn rustc_print(&self, kind: &str) -> Result<String> {
        Ok(self
            .rustc()
            .args(["--print", kind])
            .read()
            .with_context(|| format!("failed to get {kind}"))?
            .trim()
            .into())
    }

    pub(crate) fn trybuild_target_dir(&self) -> Utf8PathBuf {
        // https://github.com/dtolnay/trybuild/pull/219
        let mut trybuild_target_dir = self.metadata.target_directory.join("tests").join("trybuild");
        if !trybuild_target_dir.is_dir() {
            trybuild_target_dir.pop();
            trybuild_target_dir.push("target");
        }
        trybuild_target_dir
    }

    pub(crate) fn ui_test_target_dir(&self) -> Utf8PathBuf {
        // https://github.com/oli-obk/ui_test/blob/0.30.4/src/config.rs#L180
        self.metadata.target_directory.join("ui")
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-locate-project.html
fn locate_project(cargo: &OsStr, manifest_path: Option<&Utf8Path>) -> Result<Utf8PathBuf> {
    let mut cmd = cmd!(cargo, "locate-project");
    if let Some(manifest_path) = manifest_path {
        cmd.arg("--manifest-path");
        cmd.arg(manifest_path);
    }
    Ok(cmd.args(["--message-format", "plain"]).read()?.into())
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
// https://doc.rust-lang.org/nightly/cargo/commands/cargo-run.html
pub(crate) fn test_or_run_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if matches!(cx.args.subcommand, Subcommand::None | Subcommand::Test)
        && !cx.args.doctests
        && !cx.args.build.has_target_selection_options
    {
        cmd.arg("--tests");
    }

    for exclude in &cx.args.build.exclude_from_test {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    if !matches!(cx.args.subcommand, Subcommand::Nextest { archive_file: true }) {
        if let Some(target) = &cx.args.target {
            cmd.arg("--target");
            cmd.arg(target);
        }
        if cx.args.release {
            cmd.arg("--release");
        }
        if let Some(profile) = &cx.args.cargo_profile {
            if cx.args.subcommand.call_cargo_nextest() {
                cmd.arg("--cargo-profile");
            } else {
                cmd.arg("--profile");
            }
            cmd.arg(profile);
        }
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    // https://github.com/taiki-e/cargo-llvm-cov/issues/265
    if matches!(cx.args.subcommand, Subcommand::Nextest { archive_file: true }) {
        cmd.arg("--extract-to");
    } else {
        cmd.arg("--target-dir");
    }
    cmd.arg(cx.ws.target_dir.as_str());
    if let Some(build_dir) = &cx.ws.build_dir {
        cmd.env("CARGO_BUILD_BUILD_DIR", build_dir.as_str());
    }

    for cargo_arg in &cx.args.build.cargo_args {
        cmd.arg(cargo_arg);
    }

    if !cx.args.build.rest.is_empty() {
        cmd.arg("--");
        cmd.args(&cx.args.build.rest);
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
pub(crate) fn clean_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.args.release {
        cmd.arg("--release");
    }
    if let Some(profile) = &cx.args.cargo_profile {
        cmd.arg("--profile");
        cmd.arg(profile);
    }
    if let Some(target) = &cx.args.target {
        cmd.arg("--target");
        cmd.arg(target);
    }
    if let Some(color) = cx.ws.config.term.color {
        cmd.arg("--color");
        cmd.arg(color.as_str());
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(cx.ws.target_dir.as_str());
    if let Some(build_dir) = &cx.ws.build_dir {
        cmd.env("CARGO_BUILD_BUILD_DIR", build_dir.as_str());
    }

    cx.args.clean.cargo_args(cmd);

    // If `-vv` is passed, propagate `-v` to cargo.
    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }
}
