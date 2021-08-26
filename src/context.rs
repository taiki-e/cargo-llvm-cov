use std::{env, ffi::OsString, ops};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::{
    cargo::{self, Cargo},
    cli::Args,
    config::Config,
    env::Env,
    fs,
    process::ProcessBuilder,
    rustc::{self, Rustc},
    term,
};

pub(crate) struct Context {
    pub(crate) args: Args,
    pub(crate) env: Env,
    pub(crate) config: Config,
    pub(crate) verbose: bool,
    pub(crate) target_dir: Utf8PathBuf,
    pub(crate) doctests_dir: Utf8PathBuf,
    pub(crate) package_name: String,
    pub(crate) profdata_file: Utf8PathBuf,

    // cargo workspace info
    pub(crate) metadata: cargo_metadata::Metadata,
    // package root
    pub(crate) manifest_path: Utf8PathBuf,

    // Paths to executables.
    cargo: Cargo,
    pub(crate) llvm_cov: Utf8PathBuf,
    pub(crate) llvm_profdata: Utf8PathBuf,
}

impl Context {
    pub(crate) fn new(mut args: Args) -> Result<Self> {
        let mut env = Env::new()?;

        let package_root = cargo::package_root(args.manifest_path.as_deref())?;
        let metadata = cargo::metadata(&package_root)?;

        let cargo = Cargo::new(&env, &metadata.workspace_root)?;

        let config = Config::new(&cargo, &metadata.workspace_root)?;
        config.merge_to(&mut args, &mut env);

        term::set_coloring(&mut args.color);

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
        if args.hide_instantiations {
            warn!("--hide-instantiations option is unstable");
        }
        if args.doctests {
            warn!("--doctests option is unstable");
        }
        if args.doc {
            args.doctests = true;
            warn!("--doc option is unstable");
        }
        if args.target.is_some() {
            info!(
                "when --target option is used, coverage for proc-macro and build script will \
                 not be displayed because cargo does not pass RUSTFLAGS to them"
            );
        }
        if args.output_dir.is_none() && args.html {
            args.output_dir = Some(metadata.target_directory.join("llvm-cov"));
        }

        // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
        // use a subdirectory of the target directory as the actual target directory.
        let target_dir = metadata.target_directory.join("llvm-cov-target");
        let doctests_dir = target_dir.join("doctestbins");

        let rustc = Rustc::new(&env, &metadata.workspace_root)?;
        let sysroot = rustc::sysroot(&rustc)?;
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let rustlib = sysroot.join(format!("lib/rustlib/{}/bin", rustc.host));
        let llvm_cov = rustlib.join(format!("{}{}", "llvm-cov", env::consts::EXE_SUFFIX));
        let llvm_profdata = rustlib.join(format!("{}{}", "llvm-profdata", env::consts::EXE_SUFFIX));

        // Check if required tools are installed.
        if !llvm_cov.exists() || !llvm_profdata.exists() {
            bail!(
                "failed to find llvm-tools-preview, please install llvm-tools-preview with `rustup component add llvm-tools-preview{}`",
                if cargo.nightly { "" } else { " --toolchain nightly" }
            );
        }

        let package_name = metadata.workspace_root.file_stem().unwrap().to_string();
        let profdata_file = target_dir.join(format!("{}.profdata", package_name));

        let current_info = CargoLlvmCovInfo::current(&rustc);
        let info_file = &target_dir.join(".cargo_llvm_cov_info.json");
        let mut clean_target_dir = true;
        if info_file.is_file() {
            if let Ok(prev_info) =
                serde_json::from_str::<CargoLlvmCovInfo>(&fs::read_to_string(info_file)?)
            {
                if prev_info == current_info {
                    clean_target_dir = false;
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

        let verbose = args.verbose != 0;
        Ok(Self {
            args,
            env,
            config,
            verbose,
            target_dir,
            doctests_dir,
            package_name,
            profdata_file,
            metadata,
            manifest_path: package_root,
            cargo,
            llvm_cov,
            llvm_profdata,
        })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = cmd!(program);
        cmd.dir(&self.metadata.workspace_root);
        if self.verbose {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn cargo_process(&self) -> ProcessBuilder {
        let mut cmd = self.cargo.nightly_process();
        cmd.dir(&self.metadata.workspace_root);
        if self.verbose {
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoLlvmCovInfo {
    cargo_llvm_cov_version: String,
    rustc_version: String,
}

impl CargoLlvmCovInfo {
    fn current(rustc: &Rustc) -> Self {
        Self {
            cargo_llvm_cov_version: env!("CARGO_PKG_VERSION").into(),
            rustc_version: rustc.verbose_version.clone(),
        }
    }
}
