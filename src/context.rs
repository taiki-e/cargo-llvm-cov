use std::{ffi::OsString, path::PathBuf};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::PackageId;
use regex::Regex;

use crate::{
    cargo::Workspace,
    cli::{BuildOptions, LlvmCovOptions, ManifestOptions},
    env,
    process::ProcessBuilder,
    term,
};

pub(crate) struct Context {
    pub(crate) ws: Workspace,

    pub(crate) build: BuildOptions,
    pub(crate) cov: LlvmCovOptions,

    pub(crate) doctests: bool,
    pub(crate) no_run: bool,

    pub(crate) workspace_members: WorkspaceMembers,
    pub(crate) build_script_re: Regex,
    pub(crate) current_dir: PathBuf,

    // Paths to executables.
    pub(crate) current_exe: PathBuf,
    // Path to llvm-cov, can be overridden with `LLVM_COV` environment variable.
    pub(crate) llvm_cov: PathBuf,
    // Path to llvm-profdata, can be overridden with `LLVM_PROFDATA` environment variable.
    pub(crate) llvm_profdata: PathBuf,

    /// `CARGO_LLVM_COV_FLAGS` environment variable to pass additional flags
    /// to llvm-cov. (value: space-separated list)
    pub(crate) cargo_llvm_cov_flags: Option<String>,
    /// `CARGO_LLVM_PROFDATA_FLAGS` environment variable to pass additional flags
    /// to llvm-profdata. (value: space-separated list)
    pub(crate) cargo_llvm_profdata_flags: Option<String>,
}

impl Context {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        mut build: BuildOptions,
        manifest: &ManifestOptions,
        mut cov: LlvmCovOptions,
        exclude: &[String],
        exclude_from_report: &[String],
        doctests: bool,
        no_run: bool,
        show_env: bool,
    ) -> Result<Self> {
        let ws = Workspace::new(manifest, build.target.as_deref(), doctests, show_env)?;
        ws.config.merge_to_args(&mut build.target, &mut build.verbose, &mut build.color);
        term::set_coloring(&mut build.color);
        term::verbose::set(build.verbose != 0);

        cov.html |= cov.open;
        if cov.output_dir.is_some() && !cov.show() {
            // If the format flag is not specified, this flag is no-op.
            cov.output_dir = None;
        }
        let tmp = term::warn(); // The following warnings should not be promoted to an error.
        if cov.disable_default_ignore_filename_regex {
            warn!("--disable-default-ignore-filename-regex option is unstable");
        }
        term::warn::set(tmp);
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
        // https://github.com/rust-lang/rust/blob/1.63.0/src/bootstrap/dist.rs#L1972
        let mut rustlib: Utf8PathBuf = ws.rustc_print("target-libdir")?.into();
        rustlib.pop(); // lib
        rustlib.push("bin");
        let llvm_cov: PathBuf = match env::var_os("LLVM_COV") {
            Some(llvm_cov) => llvm_cov.into(),
            None => {
                let llvm_cov = rustlib.join(format!("{}{}", "llvm-cov", env::consts::EXE_SUFFIX));
                // Check if required tools are installed.
                if !llvm_cov.exists() {
                    let sysroot: Utf8PathBuf = ws.rustc_print("sysroot")?.into();
                    let toolchain = sysroot.file_name().unwrap();
                    // Include --toolchain flag in the suggestion because the user may be
                    // using toolchain override shorthand (+toolchain).
                    bail!(
                        "failed to find llvm-tools-preview, please install llvm-tools-preview \
                         with `rustup component add llvm-tools-preview --toolchain {}`",
                        toolchain,
                    );
                }
                llvm_cov.into()
            }
        };
        let llvm_profdata: PathBuf = match env::var_os("LLVM_PROFDATA") {
            Some(llvm_profdata) => llvm_profdata.into(),
            None => {
                let llvm_profdata =
                    rustlib.join(format!("{}{}", "llvm-profdata", env::consts::EXE_SUFFIX));
                // Check if required tools are installed.
                if !llvm_profdata.exists() {
                    let sysroot: Utf8PathBuf = ws.rustc_print("sysroot")?.into();
                    let toolchain = sysroot.file_name().unwrap();
                    // Include --toolchain flag in the suggestion because the user may be
                    // using toolchain override shorthand (+toolchain).
                    bail!(
                        "failed to find llvm-tools-preview, please install llvm-tools-preview \
                         with `rustup component add llvm-tools-preview --toolchain {}`",
                        toolchain,
                    );
                }
                llvm_profdata.into()
            }
        };

        let workspace_members = WorkspaceMembers::new(exclude, exclude_from_report, &ws.metadata);
        if workspace_members.included.is_empty() {
            bail!("no crates to be measured for coverage");
        }

        let build_script_re = pkg_hash_re(&ws, &workspace_members.included);

        Ok(Self {
            ws,
            build,
            cov,
            doctests,
            no_run,
            workspace_members,
            build_script_re,
            current_dir: env::current_dir().unwrap(),
            current_exe: match env::current_exe() {
                Ok(exe) => exe,
                Err(e) => {
                    let exe = format!("cargo-llvm-cov{}", env::consts::EXE_SUFFIX);
                    warn!("failed to get current executable, assuming {} in PATH as current executable: {}", exe, e);
                    exe.into()
                }
            },
            llvm_cov,
            llvm_profdata,
            cargo_llvm_cov_flags: env::var("CARGO_LLVM_COV_FLAGS")?,
            cargo_llvm_profdata_flags: env::var("CARGO_LLVM_PROFDATA_FLAGS")?,
        })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = cmd!(program);
        // cargo displays env vars only with -vv.
        if self.build.verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn cargo(&self) -> ProcessBuilder {
        self.ws.cargo(self.build.verbose)
    }
}

fn pkg_hash_re(ws: &Workspace, pkg_ids: &[PackageId]) -> Regex {
    let mut re = String::from("^(");
    let mut first = true;
    for id in pkg_ids {
        if first {
            first = false;
        } else {
            re.push('|');
        }
        re.push_str(&ws.metadata[id].name);
    }
    re.push_str(")-[0-9a-f]+$");
    // unwrap -- it is not realistic to have a case where there are more than
    // 5000 members in a workspace. see also pkg_hash_re_size_limit test in clean.rs.
    Regex::new(&re).unwrap()
}

pub(crate) struct WorkspaceMembers {
    pub(crate) excluded: Vec<PackageId>,
    pub(crate) included: Vec<PackageId>,
}

impl WorkspaceMembers {
    fn new(
        exclude: &[String],
        exclude_from_report: &[String],
        metadata: &cargo_metadata::Metadata,
    ) -> Self {
        let mut excluded = vec![];
        let mut included = vec![];
        if !exclude.is_empty() || !exclude_from_report.is_empty() {
            for id in &metadata.workspace_members {
                // --exclude flag doesn't handle `name:version` format
                if exclude.contains(&metadata[id].name)
                    || exclude_from_report.contains(&metadata[id].name)
                {
                    excluded.push(id.clone());
                } else {
                    included.push(id.clone());
                }
            }
        } else {
            for id in &metadata.workspace_members {
                included.push(id.clone());
            }
        }

        Self { excluded, included }
    }
}
