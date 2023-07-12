use std::ffi::OsStr;

use anyhow::{bail, format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_config2::Config;

use crate::{
    cli::{ManifestOptions, Subcommand, Args},
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

    rustc: ProcessBuilder,
    pub(crate) target_for_config: cargo_config2::TargetTriple,
    pub(crate) target_for_cli: Option<String>,
    pub(crate) nightly: bool,
    /// Whether `-C instrument-coverage` is available.
    pub(crate) stable_coverage: bool,
    /// Whether `-Z doctest-in-workspace` is needed.
    pub(crate) need_doctest_in_workspace: bool,
}

impl Workspace {
    pub(crate) fn new(
        options: &ManifestOptions,
        target: Option<&str>,
        doctests: bool,
        show_env: bool,
    ) -> Result<Self> {
        // Metadata and config
        let config = Config::load()?;
        let current_manifest = package_root(config.cargo(), options.manifest_path.as_deref())?;
        let metadata = metadata(config.cargo(), &current_manifest)?;
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
        let mut need_doctest_in_workspace = false;
        if doctests {
            need_doctest_in_workspace = cmd!(config.cargo(), "-Z", "help")
                .read()
                .map_or(false, |s| s.contains("doctest-in-workspace"))
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
            rustc,
            target_for_config,
            target_for_cli,
            nightly,
            stable_coverage,
            need_doctest_in_workspace,
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

    pub(crate) fn trybuild_target(&self) -> Utf8PathBuf {
        let mut trybuild_dir = self.metadata.target_directory.join("tests/trybuild");
        if !trybuild_dir.is_dir() {
            trybuild_dir = self.metadata.target_directory.join("tests");
        }
        let mut trybuild_target = trybuild_dir.join("target");
        // https://github.com/dtolnay/trybuild/pull/219 specifies tests/trybuild as the target
        // directory, which is a bit odd since build artifacts are generated in the same directory
        // as the test project.
        if !trybuild_target.is_dir() {
            trybuild_target.pop();
        }
        trybuild_target
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
    let nightly = channel == "nightly" || channel == "dev";
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

    add_target_dir(&cx.args, cmd, &cx.ws.target_dir);

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

    cmd.arg("--manifest-path");
    cmd.arg(&cx.ws.current_manifest);

    cmd.arg("--target-dir");
    cmd.arg(cx.ws.target_dir.as_str());

    cx.args.manifest.cargo_args(cmd);

    // If `-vv` is passed, propagate `-v` to cargo.
    if cx.args.verbose > 1 {
        cmd.arg(format!("-{}", "v".repeat(cx.args.verbose as usize - 1)));
    }
}

// https://github.com/taiki-e/cargo-llvm-cov/issues/265
fn add_target_dir(args: &Args, cmd: &mut ProcessBuilder, target_dir: &Utf8Path) {
    if args.subcommand == Subcommand::Nextest && args.cargo_args.contains(&"--archive-file".to_string()) {
        cmd.arg("--extract-to");
    } else {
        cmd.arg("--target-dir");
    }
    cmd.arg(target_dir.as_str());
}
