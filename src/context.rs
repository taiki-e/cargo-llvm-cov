use std::{collections::HashMap, convert::TryInto, env, ffi::OsString, ops};

use anyhow::{bail, format_err, Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{
    cargo::{self, Cargo},
    cli::Args,
    env::Env,
    fs,
    process::ProcessBuilder,
    term,
};

pub(crate) struct Context {
    pub(crate) args: Args,
    pub(crate) env: Env,
    pub(crate) config: cargo::Config,
    pub(crate) verbose: bool,
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
    cargo: Cargo,
    pub(crate) llvm_cov: Utf8PathBuf,
    pub(crate) llvm_profdata: Utf8PathBuf,
}

impl Context {
    pub(crate) fn new(mut args: Args) -> Result<Self> {
        let mut env = Env::new()?;

        let package_root = if let Some(manifest_path) = &args.manifest_path {
            manifest_path.clone()
        } else {
            cargo::locate_project()?.into()
        };

        let metadata =
            cargo_metadata::MetadataCommand::new().manifest_path(&package_root).exec()?;
        let cargo_target_dir = &metadata.target_directory;

        let cargo = Cargo::new(&env, &metadata.workspace_root)?;

        let config = cargo::config(&cargo, &metadata.workspace_root)?;
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
            args.output_dir = Some(cargo_target_dir.join("llvm-cov"));
        }

        // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
        // use a subdirectory of the target directory as the actual target directory.
        let target_dir = cargo_target_dir.join("llvm-cov-target");
        let doctests_dir = target_dir.join("doctestbins");

        let sysroot = sysroot(&env, cargo.nightly)?;
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let rustlib = sysroot.join(format!("lib/rustlib/{}/bin", host(&env)?));
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

        let current_info = CargoLlvmCovInfo::current();
        let info_file = &target_dir.join(".cargo_llvm_cov_info.json");
        let mut clean_target_dir = true;
        if info_file.is_file() {
            match serde_json::from_str::<CargoLlvmCovInfo>(&fs::read_to_string(info_file)?) {
                Ok(prev_info) => {
                    if prev_info == current_info {
                        clean_target_dir = false;
                    }
                }
                Err(_e) => {}
            }
        }
        if clean_target_dir {
            fs::remove_dir_all(&target_dir)?;
            fs::create_dir_all(&target_dir)?;
            fs::write(info_file, serde_json::to_string(&current_info)?)?;
            // TODO: emit info! or warn! if --no-run specified
            args.no_run = false;
        }

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
        let mut excluded = vec![];
        let mut included = vec![];
        if workspace {
            // with --workspace
            for id in &metadata.workspace_members {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();
                if args.exclude.contains(&metadata[id].name) {
                    excluded.push(manifest_dir);
                } else {
                    included.push(manifest_dir);
                }
            }
        } else if !args.package.is_empty() {
            // with --package
            for id in &metadata.workspace_members {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();
                if args.package.contains(&metadata[id].name) {
                    included.push(manifest_dir);
                } else {
                    excluded.push(manifest_dir);
                }
            }
        } else {
            let current_package = metadata.resolve.as_ref().unwrap().root.as_ref().unwrap();
            for id in &metadata.workspace_members {
                let manifest_dir = metadata[id].manifest_path.parent().unwrap();
                if current_package == id {
                    included.push(manifest_dir);
                } else {
                    excluded.push(manifest_dir);
                }
            }
        }
        let excluded_path = resolve_excluded_paths(&metadata.workspace_root, &included, &excluded);

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
            excluded_path,
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

fn resolve_excluded_paths(
    workspace_root: &Utf8Path,
    included: &[&Utf8Path],
    excluded: &[&Utf8Path],
) -> Vec<Utf8PathBuf> {
    let mut excluded_path = vec![];
    let mut contains: HashMap<&Utf8Path, Vec<_>> = HashMap::new();
    for &included in included {
        for &excluded in excluded.iter().filter(|e| included.starts_with(e)) {
            if let Some(v) = contains.get_mut(&excluded) {
                v.push(included);
            } else {
                contains.insert(excluded, vec![included]);
            }
        }
    }
    if contains.is_empty() {
        for &manifest_dir in excluded {
            let package_path = manifest_dir.strip_prefix(workspace_root).unwrap_or(manifest_dir);
            excluded_path.push(package_path.into());
        }
        return excluded_path;
    }

    for &excluded in excluded {
        let included = match contains.get(&excluded) {
            Some(included) => included,
            None => {
                let package_path = excluded.strip_prefix(workspace_root).unwrap_or(excluded);
                excluded_path.push(package_path.into());
                continue;
            }
        };

        for _ in WalkDir::new(excluded).into_iter().filter_entry(|e| {
            let p = e.path();
            if !p.is_dir() {
                if p.extension().map_or(false, |e| e == "rs") {
                    let p = p.strip_prefix(workspace_root).unwrap_or(p);
                    excluded_path.push(p.to_owned().try_into().unwrap());
                }
                return false;
            }

            let mut contains = false;
            for included in included {
                if included.starts_with(p) {
                    if p.starts_with(included) {
                        return false;
                    }
                    contains = true;
                }
            }
            if contains {
                // continue to walk
                return true;
            }
            let p = p.strip_prefix(workspace_root).unwrap_or(p);
            excluded_path.push(p.to_owned().try_into().unwrap());
            false
        }) {}
    }
    excluded_path
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

fn sysroot(env: &Env, nightly: bool) -> Result<Utf8PathBuf> {
    Ok(if nightly {
        cmd!(env.rustc(), "--print", "sysroot")
    } else {
        cmd!("rustup", "run", "nightly", "rustc", "--print", "sysroot")
    }
    .read()
    .context("failed to find sysroot")?
    .trim()
    .into())
}

fn host(env: &Env) -> Result<String> {
    let output = cmd!(env.rustc(), "--version", "--verbose").read()?;
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
