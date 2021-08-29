use std::{env, ffi::OsString, ops};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::PackageId;

use crate::{
    cargo::{self, Cargo},
    cli::Args,
    config::Config,
    env::Env,
    process::ProcessBuilder,
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
    pub(crate) workspace_members: WorkspaceMembers,

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

        // target-libdir (without --target flag) returns $sysroot/lib/rustlib/$host_triple/lib
        // llvm-tools exists in $sysroot/lib/rustlib/$host_triple/bin
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let mut rustlib: Utf8PathBuf = cargo.rustc_print("target-libdir")?.into();
        rustlib.pop(); // lib
        rustlib.push("bin");
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

        let workspace_members = WorkspaceMembers::new(&args, &metadata);
        if workspace_members.included.is_empty() {
            bail!("no crates to be measured for coverage");
        }

        let verbose = args.verbose != 0;
        let manifest_path = package_root;
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
            manifest_path,
            workspace_members,
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
        let mut cmd = self.cargo.process();
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

pub(crate) struct WorkspaceMembers {
    pub(crate) excluded: Vec<PackageId>,
    pub(crate) included: Vec<PackageId>,
}

impl WorkspaceMembers {
    fn new(args: &Args, metadata: &cargo_metadata::Metadata) -> Self {
        let workspace = args.workspace
            || (metadata.resolve.as_ref().unwrap().root.is_none() && args.package.is_empty());
        let mut excluded = vec![];
        let mut included = vec![];
        if workspace {
            // with --workspace
            for id in &metadata.workspace_members {
                // --exclude flag doesn't handle `name:version` format
                if args.exclude.contains(&metadata[id].name) {
                    excluded.push(id.clone());
                } else {
                    included.push(id.clone());
                }
            }
        } else if !args.package.is_empty() {
            // with --package
            for id in &metadata.workspace_members {
                let package = &metadata[id];
                if args.package.contains(&package.name)
                    || args.package.contains(&format!("{}:{}", &package.name, &package.version))
                {
                    included.push(id.clone());
                } else {
                    excluded.push(id.clone());
                }
            }
        } else {
            let current_package = metadata.resolve.as_ref().unwrap().root.as_ref().unwrap();
            for id in &metadata.workspace_members {
                if current_package == id {
                    included.push(id.clone());
                } else {
                    excluded.push(id.clone());
                }
            }
        }

        Self { excluded, included }
    }
}
