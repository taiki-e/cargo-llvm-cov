use std::{env, ffi::OsString};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::PackageId;

use crate::{cargo::Workspace, cli::Args, env::Env, process::ProcessBuilder, term};

pub(crate) struct Context {
    pub(crate) env: Env,
    pub(crate) ws: Workspace,

    pub(crate) args: Args,
    pub(crate) verbose: bool,

    pub(crate) workspace_members: WorkspaceMembers,

    // Paths to executables.
    pub(crate) llvm_cov: Utf8PathBuf,
    pub(crate) llvm_profdata: Utf8PathBuf,
}

impl Context {
    pub(crate) fn new(mut args: Args) -> Result<Self> {
        let mut env = Env::new()?;
        let ws = Workspace::new(&env, args.manifest_path.as_deref())?;
        ws.config.merge_to_env(&mut env);
        ws.config.merge_to_args(&mut args.target, &mut args.verbose, &mut args.color);
        term::set_coloring(&mut args.color);

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
        if args.unset_cfg_coverage {
            warn!("--unset-cfg-coverage option is unstable");
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
            args.output_dir = Some(ws.output_dir.clone());
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

        let workspace_members = WorkspaceMembers::new(&args, &ws.metadata);
        if workspace_members.included.is_empty() {
            bail!("no crates to be measured for coverage");
        }

        let verbose = args.verbose != 0;
        Ok(Self { env, ws, args, verbose, workspace_members, llvm_cov, llvm_profdata })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = cmd!(program);
        cmd.dir(&self.ws.metadata.workspace_root);
        // cargo displays env vars only with -vv.
        if self.args.verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn cargo_process(&self) -> ProcessBuilder {
        self.ws.cargo_process(self.args.verbose)
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
