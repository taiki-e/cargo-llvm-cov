#![forbid(unsafe_code)]
#![warn(future_incompatible, rust_2018_idioms, single_use_lifetimes, unreachable_pub)]
#![warn(clippy::default_trait_access, clippy::wildcard_imports)]

// Refs:
// - https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html
// - https://llvm.org/docs/CommandGuide/llvm-profdata.html
// - https://llvm.org/docs/CommandGuide/llvm-cov.html

#[macro_use]
mod trace;

#[macro_use]
mod process;

mod cli;
mod demangler;
mod fs;

use std::{
    env,
    ffi::{OsStr, OsString},
    ops,
    path::{Path, PathBuf},
};

use anyhow::{bail, format_err, Context as _, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use tracing::warn;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Opts, Subcommand},
    process::ProcessBuilder,
};

fn main() -> Result<()> {
    trace::init();

    let Opts::LlvmCov(args) = Opts::from_args();
    if let Some(Subcommand::Demangle) = &args.subcommand {
        demangler::run()?;
        return Ok(());
    }

    let cx = &Context::new(args)?;

    if let Some(output_dir) = &cx.output_dir {
        if !cx.no_run {
            fs::remove_dir_all(output_dir)?;
        }
        fs::create_dir_all(output_dir)?;
    }

    if !cx.no_run {
        for path in glob::glob(cx.target_dir.join("*.profraw").as_str())?.filter_map(Result::ok) {
            fs::remove_file(path)?;
        }

        if cx.doctests {
            fs::remove_dir_all(&cx.doctests_dir)?;
            fs::create_dir(&cx.doctests_dir)?;
        }

        fs::remove_file(&cx.profdata_file)?;
        let llvm_profile_file = cx.target_dir.join(format!("{}-%m.profraw", cx.package_name));

        let rustflags = &mut match env::var_os("RUSTFLAGS") {
            Some(rustflags) => rustflags,
            None => OsString::new(),
        };
        debug!(RUSTFLAGS = ?rustflags);
        // --remap-path-prefix for Sometimes macros are displayed with abs path
        rustflags.push(format!(
            " -Zinstrument-coverage --remap-path-prefix {}/=",
            cx.metadata.workspace_root
        ));

        // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
        let rustdocflags = &mut env::var_os("RUSTDOCFLAGS");
        debug!(RUSTDOCFLAGS = ?rustdocflags);
        if cx.doctests {
            let flags = rustdocflags.get_or_insert_with(OsString::new);
            flags.push(format!(
                " -Zinstrument-coverage -Zunstable-options --persist-doctests {}",
                cx.doctests_dir
            ));
        }

        let mut cargo = cx.process(&cx.cargo);
        if !cx.nightly {
            cargo.arg("+nightly");
        }

        cargo.env("RUSTFLAGS", &rustflags);
        cargo.env("LLVM_PROFILE_FILE", &*llvm_profile_file);
        cargo.env("CARGO_INCREMENTAL", "0");
        if let Some(rustdocflags) = rustdocflags {
            cargo.env("RUSTDOCFLAGS", &rustdocflags);
        }

        cargo.args(&["test", "--target-dir"]).arg(&cx.target_dir);
        append_args(cx, &mut cargo);

        cargo.stdout_to_stderr().run()?;
    }

    let object_files = object_files(cx)?;

    merge_profraw(cx)?;

    let format = Format::from_args(cx);
    format.run(cx, &object_files)?;

    if format == Format::Html {
        Format::None.run(cx, &object_files)?;

        if cx.open {
            open::that(Path::new(cx.output_dir.as_ref().unwrap()).join("index.html"))?;
        }
    }

    Ok(())
}

fn merge_profraw(cx: &Context) -> Result<()> {
    // Convert raw profile data.
    cx.process(&cx.llvm_profdata)
        .args(&["merge", "-sparse"])
        .args(
            glob::glob(cx.target_dir.join(format!("{}-*.profraw", cx.package_name)).as_str())?
                .filter_map(Result::ok),
        )
        .arg("-o")
        .arg(&cx.profdata_file)
        .run()?;
    Ok(())
}

fn object_files(cx: &Context) -> Result<Vec<OsString>> {
    let mut files = vec![];
    // To support testing binary crate like tests that use the CARGO_BIN_EXE
    // environment variable, pass all compiled executables.
    // This is not the ideal way, but the way unstable book says it is cannot support them.
    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#tips-for-listing-the-binaries-automatically
    let mut build_dir = cx.target_dir.join(if cx.release { "release" } else { "debug" });
    if let Some(target) = &cx.target {
        build_dir.push(target);
    }
    fs::remove_dir_all(build_dir.join("incremental"))?;
    for f in WalkDir::new(build_dir.as_str()).into_iter().filter_map(Result::ok) {
        let f = f.path();
        if is_executable::is_executable(&f) {
            files.push(f.to_owned().into_os_string())
        }
    }
    if cx.doctests {
        for f in glob::glob(cx.doctests_dir.join("*/rust_out").as_str())?.filter_map(Result::ok) {
            if is_executable::is_executable(&f) {
                files.push(f.into_os_string())
            }
        }
    }
    // This sort is necessary to make the result of `llvm-cov show` match between macos and linux.
    files.sort_unstable();
    trace!(object_files = ?files);
    Ok(files)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `llvm-cov report`
    None,
    /// `llvm-cov export -format=text`
    Json,
    /// `llvm-cov export -format=lcov`
    LCov,
    /// `llvm-cov show -format=text`
    Text,
    /// `llvm-cov show -format=html`
    Html,
}

impl Format {
    fn from_args(args: &Args) -> Self {
        if args.json {
            Self::Json
        } else if args.lcov {
            Self::LCov
        } else if args.text {
            Self::Text
        } else if args.html {
            Self::Html
        } else {
            Self::None
        }
    }

    fn llvm_cov_args(self) -> &'static [&'static str] {
        match self {
            Self::None => &["report"],
            Self::Json => &["export", "-format=text"],
            Self::LCov => &["export", "-format=lcov"],
            Self::Text => &["show", "-format=text"],
            Self::Html => &["show", "-format=html"],
        }
    }

    fn use_color(self, color: Option<Coloring>) -> Option<&'static str> {
        if matches!(self, Self::Json | Self::LCov) {
            // `llvm-cov export` doesn't have `-use-color` flag.
            // https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export
            return None;
        }
        match color {
            Some(Coloring::Auto) | None => None,
            Some(Coloring::Always) => Some("-use-color=1"),
            Some(Coloring::Never) => Some("-use-color=0"),
        }
    }

    fn run(self, cx: &Context, files: &[OsString]) -> Result<()> {
        let mut cmd = cx.process(&cx.llvm_cov);

        cmd.args(self.llvm_cov_args())
            .args(self.use_color(cx.color))
            .arg(format!("-instr-profile={}", cx.profdata_file))
            // TODO: remove `vec!`, once Rust 1.53 stable
            .args(files.iter().flat_map(|f| vec![OsStr::new("-object"), f]));

        if let Some(ignore_filename) = ignore_filename_regex(cx) {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename);
        }

        match self {
            Self::Text | Self::Html => {
                cmd.args(&[
                    "-show-instantiations",
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    &format!("-Xdemangler={}", cx.current_exe.display()),
                    "-Xdemangler=llvm-cov",
                    "-Xdemangler=demangle",
                ]);
                if let Some(output_dir) = &cx.output_dir {
                    cmd.arg(&format!("-output-dir={}", output_dir.display()));
                }
            }
            Self::Json | Self::LCov => {
                if cx.summary_only {
                    cmd.arg("-summary-only");
                }
            }
            Self::None => {}
        }

        if let Some(output_path) = &cx.output_path {
            let out = cmd.stdout_capture().read()?;
            fs::write(output_path, out)?;
            return Ok(());
        }

        cmd.run()?;
        Ok(())
    }
}

fn ignore_filename_regex(cx: &Context) -> Option<String> {
    const DEFAULT_IGNORE_FILENAME_REGEX: &str = r"rustc/|.cargo/(registry|git)/|.rustup/toolchains/|test(s)?/|examples/|benches/|target/llvm-cov-target/";

    let mut out = String::new();

    if cx.disable_default_ignore_filename_regex {
        if let Some(ignore_filename) = &cx.ignore_filename_regex {
            out.push_str(ignore_filename);
        }
    } else {
        out.push_str(DEFAULT_IGNORE_FILENAME_REGEX);
        if let Some(ignore) = &cx.ignore_filename_regex {
            out.push('|');
            out.push_str(ignore);
        }
        if let Some(home) = dirs_next::home_dir() {
            out.push('|');
            out.push_str(&home.display().to_string());
            out.push('/');
        }
    }

    for spec in &cx.exclude {
        if !cx.metadata.workspace_members.iter().any(|id| cx.metadata[id].name == *spec) {
            warn!(
                "excluded package(s) `{}` not found in workspace `{}`",
                spec, cx.metadata.workspace_root
            );
        }
    }

    for id in
        cx.metadata.workspace_members.iter().filter(|id| cx.exclude.contains(&cx.metadata[id].name))
    {
        let manifest_dir = cx.metadata[id].manifest_path.parent().unwrap();
        let package_path =
            manifest_dir.strip_prefix(&cx.metadata.workspace_root).unwrap_or(manifest_dir);
        if !out.is_empty() {
            out.push('|');
        }
        // TODO: This is still incomplete as it does not work well for patterns like `crate1/crate2`.
        out.push_str(package_path.as_str())
    }

    if out.is_empty() { None } else { Some(out) }
}

#[derive(Debug, Deserialize)]
struct Artifact {
    profile: Option<Profile>,
    #[serde(default)]
    filenames: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Profile {
    test: bool,
}

struct Context {
    args: Args,
    verbose: Option<String>,
    metadata: cargo_metadata::Metadata,
    manifest_path: PathBuf,
    target_dir: Utf8PathBuf,
    doctests_dir: Utf8PathBuf,
    package_name: String,
    profdata_file: Utf8PathBuf,
    llvm_cov: Utf8PathBuf,
    llvm_profdata: Utf8PathBuf,
    cargo: OsString,
    nightly: bool,
    current_exe: PathBuf,
}

impl Context {
    fn new(mut args: Args) -> Result<Self> {
        let verbose = if args.verbose == 0 {
            None
        } else {
            Some(format!("-{}", "v".repeat(args.verbose as _)))
        };
        debug!(?args);
        args.html |= args.open;
        if args.output_dir.is_some() && !args.show() {
            // If the format flag is not specified, this flag is no-op.
            args.output_dir = None;
        }
        if args.color.is_none() {
            // https://doc.rust-lang.org/cargo/reference/config.html#termcolor
            args.color = env::var("CARGO_TERM_COLOR").ok().map(|s| s.parse()).transpose()?;
            debug!(?args.color);
        }
        if args.disable_default_ignore_filename_regex {
            warn!("--disable-default-ignore-filename-regex is unstable");
        }
        if args.doctests {
            warn!("--doctests is unstable");
        }
        if args.no_run {
            warn!("--no-run is unstable");
        }

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

        if args.output_dir.is_none() && args.html {
            args.output_dir = Some(cargo_target_dir.join("llvm-cov").into());
        }

        // If we change RUSTFLAGS, all dependencies will be recompiled. Therefore,
        // use a subdirectory of the target directory as the actual target directory.
        let target_dir = cargo_target_dir.join("llvm-cov-target");
        let doctests_dir = target_dir.join("doctestbins");

        let mut cargo = cargo();
        let version =
            process!(&cargo, "version").dir(&metadata.workspace_root).stdout_capture().read()?;
        let nightly = version.contains("-nightly") || version.contains("-dev");
        if !nightly {
            cargo = "cargo".into();
        }

        let sysroot: Utf8PathBuf = sysroot(nightly)?.into();
        // https://github.com/rust-lang/rust/issues/85658
        // https://github.com/rust-lang/rust/blob/595088d602049d821bf9a217f2d79aea40715208/src/bootstrap/dist.rs#L2009
        let rustlib = sysroot.join(format!("lib/rustlib/{}/bin", host()?));
        let llvm_cov = rustlib.join(format!("{}{}", "llvm-cov", env::consts::EXE_SUFFIX));
        let llvm_profdata = rustlib.join(format!("{}{}", "llvm-profdata", env::consts::EXE_SUFFIX));

        debug!(?llvm_cov, ?llvm_profdata, ?cargo, ?nightly);

        // Check if required tools are installed.
        if !llvm_cov.exists() || !llvm_profdata.exists() {
            bail!(
                "failed to find llvm-tools-preview, please install llvm-tools-preview with `rustup component add llvm-tools-preview{}`",
                if !nightly { " --toolchain nightly" } else { "" }
            );
        }

        let package_name = metadata.workspace_root.file_stem().unwrap().to_string();
        let profdata_file = target_dir.join(format!("{}.profdata", package_name));

        let current_info = CargoLlvmCovInfo::new();
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
                debug!(?e);
                format!("cargo-llvm-cov{}", env::consts::EXE_SUFFIX).into()
            }
        };

        Ok(Self {
            args,
            verbose,
            metadata,
            manifest_path: package_root,
            target_dir,
            doctests_dir,
            package_name,
            profdata_file,
            llvm_cov,
            llvm_profdata,
            cargo,
            nightly,
            current_exe,
        })
    }

    fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoLlvmCovInfo {
    version: String,
}

impl CargoLlvmCovInfo {
    fn new() -> Self {
        Self { version: env!("CARGO_PKG_VERSION").into() }
    }
}

fn sysroot(nightly: bool) -> Result<String> {
    Ok(if nightly {
        process!(rustc(), "--print", "sysroot")
    } else {
        process!("rustup", "run", "nightly", "rustc", "--print", "sysroot")
    }
    .stdout_capture()
    .read()
    .context("failed to find sysroot")?
    .trim()
    .into())
}

fn host() -> Result<String> {
    let rustc = &rustc();
    let output = process!(rustc, "--version", "--verbose").stdout_capture().read()?;
    output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| {
            format_err!("unexpected version output from `{}`: {}", rustc.to_string_lossy(), output)
        })
        .map(ToString::to_string)
}

fn rustc() -> OsString {
    env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"))
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn append_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.no_fail_fast {
        cmd.arg("--no-fail-fast");
    }
    if cx.workspace {
        cmd.arg("--workspace");
    }
    for exclude in &cx.exclude {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    if cx.release {
        cmd.arg("--release");
    }
    for features in &cx.features {
        cmd.arg("--features");
        cmd.arg(features);
    }
    if cx.all_features {
        cmd.arg("--all-features");
    }
    if cx.no_default_features {
        cmd.arg("--no-default-features");
    }
    if let Some(target) = &cx.target {
        cmd.arg("--target");
        cmd.arg(target);
    }

    cmd.arg("--manifest-path");
    cmd.arg(&cx.manifest_path);

    if let Some(color) = cx.color {
        cmd.arg("--color");
        cmd.arg(color.cargo_color());
    }
    if cx.frozen {
        cmd.arg("--frozen");
    }
    if cx.locked {
        cmd.arg("--locked");
    }

    if let Some(verbose) = &cx.verbose {
        cmd.arg(verbose);
    }

    for unstable_flag in &cx.unstable_flags {
        cmd.arg("-Z");
        cmd.arg(unstable_flag);
    }

    if !cx.args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&cx.args.args);
    }
}
