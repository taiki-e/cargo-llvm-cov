use std::{ffi::OsStr, path::PathBuf};

use anyhow::{bail, format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_config2::Config;

use crate::{
    cli::{ManifestOptions, Subcommand},
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
    rustc: ProcessBuilder,
    pub(crate) target_for_config: cargo_config2::TargetTriple,
    pub(crate) target_for_cli: Option<String>,
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

        // Metadata and config
        let current_manifest = package_root(&cargo, options.manifest_path.as_deref())?;
        let metadata = metadata(&cargo, &current_manifest)?;
        let config = Config::load()?;
        let mut target_for_config = config.build_target_for_config(target)?;
        if target_for_config.len() != 1 {
            bail!("cargo-llvm-cov doesn't currently supports multi-target builds: {target_for_config:?}");
        }
        let target_for_config = target_for_config.pop().unwrap();
        let target_for_cli = config.build_target_for_cli(target)?.pop();
        let rustc = ProcessBuilder::from(config.rustc().clone());
        let nightly = rustc_version(&rustc)?;

        if doctests && !nightly {
            bail!("--doctests flag requires nightly toolchain; consider using `cargo +nightly llvm-cov`")
        }
        let stable_coverage =
            rustc.clone().args(["-C", "help"]).read()?.contains("instrument-coverage");
        if !stable_coverage && !nightly {
            bail!(
                "cargo-llvm-cov requires rustc 1.60+; consider updating toolchain (`rustup update`)
                 or using nightly toolchain (`cargo +nightly llvm-cov`)"
            );
        }

        let target_dir =
            if let Some(path) = env::var("CARGO_LLVM_COV_TARGET_DIR")?.map(Utf8PathBuf::from) {
                let mut base: Utf8PathBuf = env::current_dir()?.try_into()?;
                base.push(path);
                base
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
        let profdata_file = target_dir.join(format!("{name}.profdata"));

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
            target_for_config,
            target_for_cli,
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
}

fn rustc_version(rustc: &ProcessBuilder) -> Result<bool> {
    let mut cmd = rustc.clone();
    cmd.args(["--version", "--verbose"]);
    let verbose_version = cmd.read()?;
    let version = verbose_version
        .lines()
        .find_map(|line| line.strip_prefix("release: "))
        .ok_or_else(|| format_err!("unexpected version output from `{cmd}`: {verbose_version}"))?;
    let (_version, channel) = version.split_once('-').unwrap_or_default();
    let nightly = channel == "nightly" || version == "dev";
    Ok(nightly)
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
fn metadata(cargo: &OsStr, manifest_path: &Utf8Path) -> Result<cargo_metadata::Metadata> {
    let mut cmd = cmd!(cargo, "metadata", "--format-version=1", "--manifest-path", manifest_path);
    serde_json::from_str(&cmd.read()?).with_context(|| format!("failed to parse output from {cmd}"))
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
// https://doc.rust-lang.org/nightly/cargo/commands/cargo-run.html
pub(crate) fn test_or_run_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if matches!(cx.args.subcommand, Subcommand::None | Subcommand::Test) && !cx.args.doctests {
        let has_target_selection_options = cx.args.lib
            | cx.args.bins
            | cx.args.examples
            | cx.args.tests
            | cx.args.benches
            | cx.args.all_targets
            | cx.args.doc
            | !cx.args.bin.is_empty()
            | !cx.args.example.is_empty()
            | !cx.args.test.is_empty()
            | !cx.args.bench.is_empty();
        if !has_target_selection_options {
            cmd.arg("--tests");
        }
    }

    for exclude in &cx.args.exclude_from_test {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(&cx.ws.target_dir);

    for cargo_arg in &cx.args.cargo_args {
        cmd.arg(cargo_arg);
    }

    if !cx.args.rest.is_empty() {
        cmd.arg("--");
        cmd.args(&cx.args.rest);
    }
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
pub(crate) fn clean_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.args.release {
        cmd.arg("--release");
    }
    // nextest's --profile option is different from cargo.
    if cx.args.subcommand != Subcommand::Nextest {
        if let Some(profile) = &cx.args.profile {
            cmd.arg("--profile");
            cmd.arg(profile);
        }
    }
    if let Some(target) = &cx.args.target {
        cmd.arg("--target");
        cmd.arg(target);
    }
    if let Some(color) = cx.args.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(&cx.ws.target_dir);

    cx.args.manifest.cargo_args(cmd);

    // If `-vv` is passed, propagate `-v` to cargo.
    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }
}
