use std::{env, ffi::OsString};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::PackageId;

use crate::{
    cargo::Workspace,
    cli::{BuildOptions, LlvmCovOptions, ManifestOptions},
    env::Env,
    process::ProcessBuilder,
    term,
};

pub(crate) struct Context {
    pub(crate) env: Env,
    pub(crate) ws: Workspace,

    pub(crate) build: BuildOptions,
    pub(crate) manifest: ManifestOptions,
    pub(crate) cov: LlvmCovOptions,

    pub(crate) verbose: bool,
    pub(crate) quiet: bool,
    pub(crate) doctests: bool,
    pub(crate) no_run: bool,

    pub(crate) workspace_members: WorkspaceMembers,

    // Paths to executables.
    pub(crate) llvm_cov: Utf8PathBuf,
    pub(crate) llvm_profdata: Utf8PathBuf,
}

impl Context {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        mut build: BuildOptions,
        manifest: ManifestOptions,
        mut cov: LlvmCovOptions,
        workspace: bool,
        exclude: &[String],
        package: &[String],
        quiet: bool,
        doctests: bool,
        no_run: bool,
    ) -> Result<Self> {
        let env = Env::new()?;
        let ws = Workspace::new(&env, &manifest, build.target.as_deref())?;
        ws.config.merge_to_args(&mut build.target, &mut build.verbose, &mut build.color);
        term::set_coloring(&mut build.color);

        cov.html |= cov.open;
        if cov.output_dir.is_some() && !cov.show() {
            // If the format flag is not specified, this flag is no-op.
            cov.output_dir = None;
        }
        if cov.disable_default_ignore_filename_regex {
            warn!("--disable-default-ignore-filename-regex option is unstable");
        }
        if cov.hide_instantiations {
            warn!("--hide-instantiations option is unstable");
        }
        if cov.no_cfg_coverage {
            warn!("--no-cfg-coverage option is unstable");
        }
        if build.target.is_some() {
            info!(
                "when --target option is used, coverage for proc-macro and build script will \
                 not be displayed because cargo does not pass RUSTFLAGS to them"
            );
        }
        if cov.output_dir.is_none() && cov.html {
            cov.output_dir = Some(ws.output_dir.clone());
        }

        // target-libdir (without --target flag) returns $sysroot/lib/rustlib/$host_triple/lib
        // llvm-tools exists in $sysroot/lib/rustlib/$host_triple/bin
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let mut rustlib: Utf8PathBuf = ws.rustc_print("target-libdir")?.into();
        rustlib.pop(); // lib
        rustlib.push("bin");
        let llvm_cov = rustlib.join(format!("{}{}", "llvm-cov", env::consts::EXE_SUFFIX));
        let llvm_profdata = rustlib.join(format!("{}{}", "llvm-profdata", env::consts::EXE_SUFFIX));

        // Check if required tools are installed.
        if !llvm_cov.exists() || !llvm_profdata.exists() {
            bail!(
                "failed to find llvm-tools-preview, please install llvm-tools-preview with `rustup component add llvm-tools-preview{}`",
                if ws.cargo.nightly { "" } else { " --toolchain nightly" }
            );
        }

        let workspace_members = WorkspaceMembers::new(workspace, exclude, package, &ws.metadata);
        if workspace_members.included.is_empty() {
            bail!("no crates to be measured for coverage");
        }

        let verbose = build.verbose != 0;
        Ok(Self {
            env,
            ws,
            build,
            manifest,
            cov,
            verbose,
            quiet,
            doctests,
            no_run,
            workspace_members,
            llvm_cov,
            llvm_profdata,
        })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = cmd!(program);
        cmd.dir(&self.ws.metadata.workspace_root);
        // cargo displays env vars only with -vv.
        if self.build.verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn cargo_process(&self) -> ProcessBuilder {
        self.ws.cargo_process(self.build.verbose)
    }
}

pub(crate) struct WorkspaceMembers {
    pub(crate) excluded: Vec<PackageId>,
    pub(crate) included: Vec<PackageId>,
}

impl WorkspaceMembers {
    fn new(
        workspace: bool,
        exclude: &[String],
        package: &[String],
        metadata: &cargo_metadata::Metadata,
    ) -> Self {
        let workspace =
            workspace || (metadata.resolve.as_ref().unwrap().root.is_none() && package.is_empty());
        let mut excluded = vec![];
        let mut included = vec![];
        if workspace {
            // with --workspace
            for id in &metadata.workspace_members {
                // --exclude flag doesn't handle `name:version` format
                if exclude.contains(&metadata[id].name) {
                    excluded.push(id.clone());
                } else {
                    included.push(id.clone());
                }
            }
        } else if !package.is_empty() {
            // with --package
            for id in &metadata.workspace_members {
                let pkg = &metadata[id];
                if package.contains(&pkg.name)
                    || package.contains(&format!("{}:{}", &pkg.name, &pkg.version))
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
