use std::{
    env,
    ffi::{OsStr, OsString},
    ops,
    path::PathBuf,
};

use anyhow::{bail, format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::{cargo, cli::Args, fs, process, process::ProcessBuilder, term};

pub(crate) struct Context {
    pub(crate) args: Args,
    pub(crate) env: EnvironmentVariables,
    pub(crate) verbose: Option<String>,
    pub(crate) target_dir: Utf8PathBuf,
    pub(crate) doctests_dir: Utf8PathBuf,
    pub(crate) package_name: String,
    pub(crate) profdata_file: Utf8PathBuf,

    // cargo workspace info
    pub(crate) metadata: cargo_metadata::Metadata,
    // package root
    pub(crate) manifest_path: Utf8PathBuf,
    pub(crate) excluded_path: Vec<Utf8PathBuf>,

    // Paths to executables.
    pub(crate) cargo: Cargo,
    pub(crate) llvm_cov: Utf8PathBuf,
    pub(crate) llvm_profdata: Utf8PathBuf,
    pub(crate) current_exe: PathBuf,
}

impl Context {
    pub(crate) fn new(mut args: Args) -> Result<Self> {
        let mut env = EnvironmentVariables::new();
        debug!(?args);
        debug!(?env);

        let package_root = if let Some(manifest_path) = &args.manifest_path {
            manifest_path.clone()
        } else {
            process!("cargo", "locate-project", "--message-format", "plain")
                .stdout_capture()
                .read()?
                .into()
        };

        let metadata =
            cargo_metadata::MetadataCommand::new().manifest_path(&package_root).exec()?;
        let cargo_target_dir = &metadata.target_directory;
        debug!(?package_root, ?metadata.workspace_root, ?metadata.target_directory);

        // .cargo/config is prefer over .cargo/config.toml
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure
        let workspace_config = &metadata.workspace_root.join(".cargo/config");
        let workspace_config_toml = &metadata.workspace_root.join(".cargo/config.toml");
        let mut config = if workspace_config.is_file() {
            toml::from_str(&fs::read_to_string(workspace_config)?)?
        } else if workspace_config_toml.is_file() {
            toml::from_str(&fs::read_to_string(workspace_config_toml)?)?
        } else {
            cargo::Config::default()
        };
        config.apply_env()?;
        config.merge_to(&mut args, &mut env);

        term::set_coloring(args.color);

        if let Some(v) = env::var_os("LLVM_PROFILE_FILE") {
            warn!("environment variable LLVM_PROFILE_FILE={:?} will be ignored", v);
            env::remove_var("LLVM_PROFILE_FILE");
        }

        args.html |= args.open;
        if args.output_dir.is_some() && !args.show() {
            // If the format flag is not specified, this flag is no-op.
            args.output_dir = None;
        }
        if args.disable_default_ignore_filename_regex {
            warn!("--disable-default-ignore-filename-regex option is unstable");
        }
        if args.doctests {
            warn!("--doctests option is unstable");
        }
        if args.no_run {
            warn!("--no-run option is unstable");
        }
        if args.target.is_some() {
            warn!(
                "When --target option is used, coverage for proc-macro and build script will \
                 not be displayed because cargo does not pass RUSTFLAGS to them"
            );
        }
        let verbose = if args.verbose == 0 {
            None
        } else {
            Some(format!("-{}", "v".repeat(args.verbose as _)))
        };
        if args.output_dir.is_none() && args.html {
            args.output_dir = Some(cargo_target_dir.join("llvm-cov"));
        }

        // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
        // use a subdirectory of the target directory as the actual target directory.
        let target_dir = cargo_target_dir.join("llvm-cov-target");
        let doctests_dir = target_dir.join("doctestbins");

        let cargo = Cargo::new(&env, &metadata.workspace_root)?;
        debug!(?cargo);

        let sysroot: Utf8PathBuf = sysroot(&env, cargo.nightly)?.into();
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let rustlib = sysroot.join(format!("lib/rustlib/{}/bin", host(&env)?));
        let llvm_cov = rustlib.join(format!("{}{}", "llvm-cov", env::consts::EXE_SUFFIX));
        let llvm_profdata = rustlib.join(format!("{}{}", "llvm-profdata", env::consts::EXE_SUFFIX));
        debug!(?llvm_cov, ?llvm_profdata);

        // Check if required tools are installed.
        if !llvm_cov.exists() || !llvm_profdata.exists() {
            bail!(
                "failed to find llvm-tools-preview, please install llvm-tools-preview with `rustup component add llvm-tools-preview{}`",
                if cargo.nightly { "" } else { " --toolchain nightly" }
            );
        }

        let package_name = metadata.workspace_root.file_stem().unwrap().to_string();
        let profdata_file = target_dir.join(format!("{}.profdata", package_name));

        let current_info = CargoLlvmCovInfo::current();
        debug!(?current_info);
        let info_file = &target_dir.join(".cargo_llvm_cov_info.json");
        let mut clean_target_dir = true;
        if info_file.is_file() {
            match serde_json::from_str::<CargoLlvmCovInfo>(&fs::read_to_string(info_file)?) {
                Ok(prev_info) => {
                    debug!(?prev_info);
                    if prev_info == current_info {
                        clean_target_dir = false;
                    }
                }
                Err(e) => {
                    debug!(?e);
                }
            }
        }
        if clean_target_dir {
            fs::remove_dir_all(&target_dir)?;
            fs::create_dir_all(&target_dir)?;
            fs::write(info_file, serde_json::to_string(&current_info)?)?;
            // TODO: emit info! or warn! if --no-run specified
            args.no_run = false;
        }
        let current_exe = match env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                let exe = format!("cargo-llvm-cov{}", env::consts::EXE_SUFFIX);
                warn!("failed to get current executable, assuming {} in PATH as current executable: {}", exe, e);
                exe.into()
            }
        };

        let mut excluded_path = vec![];
        for spec in &args.exclude {
            if !metadata.workspace_members.iter().any(|id| metadata[id].name == *spec) {
                warn!(
                    "excluded package(s) `{}` not found in workspace `{}`",
                    spec, metadata.workspace_root
                );
            }
        }
        let workspace = args.workspace
            || (metadata.resolve.as_ref().unwrap().root.is_none() && args.package.is_empty());
        if workspace {
            // with --workspace
            for id in metadata
                .workspace_members
                .iter()
                .filter(|id| args.exclude.contains(&metadata[id].name))
            {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();

                let package_path =
                    manifest_dir.strip_prefix(&metadata.workspace_root).unwrap_or(manifest_dir);
                // TODO: This is still incomplete as it does not work well for patterns like `crate1/crate2`.
                excluded_path.push(package_path.into());
            }
        } else if !args.package.is_empty() {
            // with --package
            for id in metadata
                .workspace_members
                .iter()
                .filter(|id| !args.package.contains(&metadata[*id].name))
            {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();

                let package_path =
                    manifest_dir.strip_prefix(&metadata.workspace_root).unwrap_or(manifest_dir);
                // TODO: This is still incomplete as it does not work well for patterns like `crate1/crate2`.
                excluded_path.push(package_path.into());
            }
        } else {
            let current_package = metadata.resolve.as_ref().unwrap().root.as_ref().unwrap();
            for id in metadata.workspace_members.iter().filter(|id| **id != *current_package) {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();

                let package_path =
                    manifest_dir.strip_prefix(&metadata.workspace_root).unwrap_or(manifest_dir);
                // TODO: This is still incomplete as it does not work well for patterns like `crate1/crate2`.
                excluded_path.push(package_path.into());
            }
        }

        Ok(Self {
            args,
            verbose,
            target_dir,
            doctests_dir,
            package_name,
            profdata_file,
            metadata,
            manifest_path: package_root,
            excluded_path,
            cargo,
            llvm_cov,
            llvm_profdata,
            current_exe,
            env,
        })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = process!(program);
        cmd.dir(&self.metadata.workspace_root);
        if self.verbose.is_some() {
            cmd.display_env_vars();
        }
        cmd
    }
}

impl ops::Deref for Context {
    type Target = Args;

    fn deref(&self) -> &Self::Target {
        &self.args
    }
}

/// Environment variables fetched at runtime.
///
/// These environment variables are either applied to both cargo and
/// cargo-llvm-cov, or are modified before being passed to cargo.
#[derive(Debug)]
pub(crate) struct EnvironmentVariables {
    // Environment variables Cargo reads
    // https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-reads
    /// `RUSTFLAGS` environment variable.
    pub(crate) rustflags: Option<OsString>,
    /// `RUSTDOCFLAGS` environment variable.
    pub(crate) rustdocflags: Option<OsString>,
    /// `RUSTC` environment variable.
    pub(crate) rustc: Option<OsString>,

    // Environment variables Cargo sets for 3rd party subcommands
    // https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-3rd-party-subcommands
    /// `CARGO` environment variable.
    pub(crate) cargo: Option<OsString>,
}

impl EnvironmentVariables {
    fn new() -> Self {
        env::set_var("CARGO_INCREMENTAL", "0");
        Self {
            rustflags: env::var_os("RUSTFLAGS"),
            rustdocflags: env::var_os("RUSTDOCFLAGS"),
            rustc: env::var_os("RUSTC"),
            cargo: env::var_os("CARGO"),
        }
    }

    pub(crate) fn rustc(&self) -> &OsStr {
        self.rustc.as_deref().unwrap_or_else(|| OsStr::new("rustc"))
    }

    pub(crate) fn cargo(&self) -> &OsStr {
        self.cargo.as_deref().unwrap_or_else(|| OsStr::new("cargo"))
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoLlvmCovInfo {
    version: String,
}

impl CargoLlvmCovInfo {
    fn current() -> Self {
        Self { version: env!("CARGO_PKG_VERSION").into() }
    }
}

#[derive(Debug)]
pub(crate) struct Cargo {
    path: OsString,
    pub(crate) nightly: bool,
}

impl Cargo {
    pub(crate) fn new(env: &EnvironmentVariables, workspace_root: &Utf8Path) -> Result<Self> {
        let mut path = env.cargo().to_owned();
        let version = process!(&path, "version").dir(workspace_root).stdout_capture().read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");
        if !nightly {
            path = "cargo".into();
        }

        Ok(Self { path, nightly })
    }
}

impl ops::Deref for Cargo {
    type Target = OsString;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn sysroot(env: &EnvironmentVariables, nightly: bool) -> Result<String> {
    Ok(if nightly {
        process!(env.rustc(), "--print", "sysroot")
    } else {
        process!("rustup", "run", "nightly", "rustc", "--print", "sysroot")
    }
    .stdout_capture()
    .read()
    .context("failed to find sysroot")?
    .trim()
    .into())
}

fn host(env: &EnvironmentVariables) -> Result<String> {
    let output = process!(env.rustc(), "--version", "--verbose").stdout_capture().read()?;
    output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| {
            format_err!(
                "unexpected version output from `{}`: {}",
                env.rustc().to_string_lossy(),
                output
            )
        })
        .map(str::to_owned)
}
