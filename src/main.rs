#![forbid(unsafe_code)]
#![warn(future_incompatible, rust_2018_idioms, single_use_lifetimes, unreachable_pub)]
#![warn(clippy::default_trait_access, clippy::wildcard_imports)]

// Refs:
// - https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html
// - https://llvm.org/docs/CommandGuide/llvm-profdata.html
// - https://llvm.org/docs/CommandGuide/llvm-cov.html

#[macro_use]
mod term;

#[macro_use]
mod process;

mod cargo;
mod cli;
mod config;
mod context;
mod demangler;
mod env;
mod fs;

use std::{
    collections::HashMap,
    convert::TryInto,
    ffi::{OsStr, OsString},
};

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Clap;
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Opts, Subcommand},
    config::StringOrArray,
    context::Context,
    env::Env,
};

fn main() {
    if let Err(e) = try_main() {
        error!("{:#}", e);
        std::process::exit(1)
    }
}

fn try_main() -> Result<()> {
    let Opts::LlvmCov(args) = Opts::parse();

    match args.subcommand {
        Some(Subcommand::Demangle) => {
            demangler::run()?;
        }

        Some(Subcommand::Clean { manifest_path, verbose, mut color }) => {
            term::set_coloring(&mut color);
            let env = Env::new()?;
            let package_root = cargo::package_root(&env, manifest_path.as_deref())?;
            let metadata = cargo::metadata(&env, &package_root)?;

            let target_dir = metadata.target_directory.join("llvm-cov-target");
            let output_dir = metadata.target_directory.join("llvm-cov");
            for dir in &[target_dir, output_dir] {
                if dir.exists() {
                    if verbose != 0 {
                        status!("Removing", "{}", dir);
                    }
                    fs::remove_dir_all(dir)?;
                }
            }
        }

        None => {
            term::set_quiet(args.quiet);
            let cx = &Context::new(args)?;

            clean_partial(cx)?;
            create_dirs(cx)?;
            match (cx.no_run, cx.no_report) {
                (false, false) => {
                    run_test(cx)?;
                    generate_report(cx)?;
                }
                (false, true) => {
                    run_test(cx)?;
                }
                (true, false) => {
                    generate_report(cx)?;
                }
                (true, true) => unreachable!(),
            }
        }
    }
    Ok(())
}

// Heuristic to avoid false positives/false negatives:
// - If --no-run or --no-report is used: do not remove artifacts
// - Otherwise, remove the followings:
//   - build artifacts of crates to be measured for coverage
//   - profdata
//   - profraw
//   - doctest bins
//   - old reports
fn clean_partial(cx: &Context) -> Result<()> {
    if cx.no_run || cx.no_report {
        return Ok(());
    }

    if let Some(output_dir) = &cx.output_dir {
        for format in &["html", "text"] {
            fs::remove_dir_all(output_dir.join(format))?;
        }
    }

    let package_args: Vec<_> = cx
        .workspace_members
        .included
        .iter()
        .flat_map(|id| ["--package", &cx.metadata[id].name])
        .collect();
    let mut cmd = cx.cargo_process();
    cmd.args(["clean", "--target-dir", cx.target_dir.as_str()]).args(&package_args);
    cargo::clean_args(cx, &mut cmd);
    if let Err(e) = cmd.run_with_output() {
        warn!("{:#}", e);
    }
    // trybuild
    let trybuild_dir = &cx.target_dir.join("tests");
    let trybuild_target = trybuild_dir.join("target");
    for metadata in trybuild_metadata(cx)? {
        let mut cmd = cx.cargo_process();
        cmd.args(["clean", "--target-dir", trybuild_target.as_str()]).args(&package_args);
        cargo::clean_args(cx, &mut cmd);
        if let Err(_e) = cmd.dir(metadata.workspace_root).run_with_output() {
            // We don't know if all included packages are referenced in the
            // trybuild test, so we ignore the error.
        }
    }

    for path in glob::glob(cx.target_dir.join("*.profraw").as_str())?.filter_map(Result::ok) {
        fs::remove_file(path)?;
    }

    if cx.doctests {
        fs::remove_dir_all(&cx.doctests_dir)?;
    }

    fs::remove_file(&cx.profdata_file)?;
    Ok(())
}

fn create_dirs(cx: &Context) -> Result<()> {
    fs::create_dir_all(&cx.target_dir)?;

    if let Some(output_dir) = &cx.output_dir {
        fs::create_dir_all(output_dir)?;
        if cx.html {
            fs::create_dir_all(output_dir.join("html"))?;
        }
        if cx.text {
            fs::create_dir_all(output_dir.join("text"))?;
        }
    }

    if cx.doctests {
        fs::create_dir_all(&cx.doctests_dir)?;
    }
    Ok(())
}

fn run_test(cx: &Context) -> Result<()> {
    let llvm_profile_file = cx.target_dir.join(format!("{}-%m.profraw", cx.package_name));

    let rustflags = &mut cx.env.rustflags.clone().unwrap_or_default();
    rustflags.push(" -Z instrument-coverage");
    if !cx.unset_cfg_coverage {
        rustflags.push(" --cfg coverage");
    }
    // --remap-path-prefix is needed because sometimes macros are displayed with absolute path
    rustflags.push(format!(" --remap-path-prefix {}/=", cx.metadata.workspace_root));
    if cx.target.is_none() {
        rustflags.push(" --cfg trybuild_no_target");
    }

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    let rustdocflags = &mut cx.env.rustdocflags.clone();
    if cx.doctests {
        let flags = rustdocflags.get_or_insert_with(OsString::new);
        flags.push(format!(
            " -Z instrument-coverage -Z unstable-options --persist-doctests {}",
            cx.doctests_dir
        ));
    }

    let mut cargo = cx.cargo_process();
    cargo.env("RUSTFLAGS", &rustflags);
    if let Some(rustdocflags) = rustdocflags {
        cargo.env("RUSTDOCFLAGS", &rustdocflags);
    }
    cargo.env("LLVM_PROFILE_FILE", &*llvm_profile_file);
    cargo.env("CARGO_INCREMENTAL", "0");
    cargo.env("CARGO_TARGET_DIR", &cx.target_dir);

    cargo.arg("test");
    if cx.doctests && !cx.unstable_flags.iter().any(|f| f == "doctest-in-workspace") {
        // https://github.com/rust-lang/cargo/issues/9427
        cargo.arg("-Z");
        cargo.arg("doctest-in-workspace");
    }
    cargo::test_args(cx, &mut cargo);

    if cx.verbose {
        status!("Running", "{}", cargo);
    }
    cargo.stdout_to_stderr().run()?;
    Ok(())
}

fn generate_report(cx: &Context) -> Result<()> {
    merge_profraw(cx).context("failed to merge profile data")?;

    let object_files = object_files(cx).context("failed to collect object files")?;
    let ignore_filename_regex = ignore_filename_regex(cx);
    for format in Format::from_args(cx) {
        format
            .generate_report(cx, &object_files, ignore_filename_regex.as_ref())
            .context("failed to generate report")?;
    }

    if cx.open {
        let path = &cx.output_dir.as_ref().unwrap().join("html/index.html");
        status!("Opening", "{}", path);
        open_report(cx, path)?;
    }
    Ok(())
}

fn open_report(cx: &Context, path: &Utf8Path) -> Result<()> {
    // doc.browser config value is prefer over BROWSER environment variable.
    // https://github.com/rust-lang/cargo/blob/0.55.0/src/cargo/ops/cargo_doc.rs#L58-L59
    let browser = cx
        .config
        .doc
        .browser
        .as_ref()
        .and_then(StringOrArray::path_and_args)
        .or_else(|| Some((cx.env.browser.as_deref()?, vec![])));

    match browser {
        Some((browser, initial_args)) => {
            cmd!(browser).args(initial_args).arg(path).run().with_context(|| {
                format!("couldn't open report with {}", browser.to_string_lossy())
            })?;
        }
        None => opener::open(path).context("couldn't open report")?,
    }
    Ok(())
}

fn merge_profraw(cx: &Context) -> Result<()> {
    // Convert raw profile data.
    let mut cmd = cx.process(&cx.llvm_profdata);
    cmd.args(["merge", "-sparse"])
        .args(
            glob::glob(cx.target_dir.join(format!("{}-*.profraw", cx.package_name)).as_str())?
                .filter_map(Result::ok),
        )
        .arg("-o")
        .arg(&cx.profdata_file);
    if let Some(jobs) = cx.jobs {
        cmd.arg(format!("-num-threads={}", jobs));
    }
    if let Some(flags) = &cx.env.cargo_llvm_profdata_flags {
        cmd.args(flags.split(' ').filter(|s| !s.trim().is_empty()));
    }
    if cx.verbose {
        status!("Running", "{}", cmd);
    }
    cmd.stdout_to_stderr().run()?;
    Ok(())
}

fn object_files(cx: &Context) -> Result<Vec<OsString>> {
    fn walk_target_dir(target_dir: &Utf8Path) -> impl Iterator<Item = walkdir::DirEntry> {
        WalkDir::new(target_dir.as_str())
            .into_iter()
            .filter_entry(|e| {
                let p = e.path();
                if p.is_dir()
                    && p.file_name().map_or(false, |f| f == "incremental" || f == ".fingerprint")
                {
                    return false;
                }
                true
            })
            .filter_map(Result::ok)
    }

    let mut files = vec![];
    // To support testing binary crate like tests that use the CARGO_BIN_EXE
    // environment variable, pass all compiled executables.
    // This is not the ideal way, but the way unstable book says it is cannot support them.
    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#tips-for-listing-the-binaries-automatically
    let mut target_dir = cx.target_dir.clone();
    // https://doc.rust-lang.org/nightly/cargo/guide/build-cache.html
    if let Some(target) = &cx.target {
        target_dir.push(target);
    }
    target_dir.push(if cx.release { "release" } else { "debug" });
    for f in walk_target_dir(&target_dir) {
        let f = f.path();
        if is_executable::is_executable(&f) {
            files.push(f.to_owned().into_os_string());
        }
    }
    if cx.doctests {
        for f in glob::glob(cx.doctests_dir.join("*/rust_out").as_str())?.filter_map(Result::ok) {
            if is_executable::is_executable(&f) {
                files.push(f.into_os_string());
            }
        }
    }

    // trybuild
    let trybuild_dir = &cx.target_dir.join("tests");
    let mut trybuild_target = trybuild_dir.join("target");
    if let Some(target) = &cx.target {
        trybuild_target.push(target);
    }
    // Currently, trybuild always use debug build.
    trybuild_target.push("debug");
    if trybuild_target.is_dir() {
        let mut trybuild_targets = vec![];
        for metadata in trybuild_metadata(cx)? {
            for package in metadata.packages {
                for target in package.targets {
                    trybuild_targets.push(target.name);
                }
            }
        }
        if !trybuild_targets.is_empty() {
            let re = Regex::new(&format!("^({})-[0-9a-f]+$", trybuild_targets.join("|"))).unwrap();
            for entry in walk_target_dir(&trybuild_target) {
                let path = entry.path();
                if let Some(path) = path.file_name().unwrap().to_str() {
                    if re.is_match(path) {
                        continue;
                    }
                }
                if is_executable::is_executable(path) {
                    files.push(path.to_owned().into_os_string());
                }
            }
        }
    }

    // This sort is necessary to make the result of `llvm-cov show` match between macos and linux.
    files.sort_unstable();
    Ok(files)
}

/// Collects metadata for packages generated by trybuild. If the trybuild test
/// directory is not found, it returns an empty vector.
fn trybuild_metadata(cx: &Context) -> Result<Vec<cargo_metadata::Metadata>> {
    let trybuild_dir = &cx.target_dir.join("tests");
    if !trybuild_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut metadata = vec![];
    for entry in fs::read_dir(trybuild_dir)?.filter_map(Result::ok) {
        let manifest_path = entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        metadata.push(
            cargo_metadata::MetadataCommand::new().manifest_path(manifest_path).no_deps().exec()?,
        );
    }
    Ok(metadata)
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
    fn from_args(args: &Args) -> Vec<Self> {
        if args.json {
            vec![Self::Json]
        } else if args.lcov {
            vec![Self::LCov]
        } else if args.text {
            vec![Self::Text]
        } else if args.html {
            vec![Self::Html]
        } else {
            vec![Self::None]
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

    fn use_color(self, cx: &Context) -> Option<&'static str> {
        if matches!(self, Self::Json | Self::LCov) {
            // `llvm-cov export` doesn't have `-use-color` flag.
            // https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export
            return None;
        }
        if self == Self::Text && cx.output_dir.is_some() {
            return Some("-use-color=0");
        }
        match cx.color {
            Some(Coloring::Auto) | None => None,
            Some(Coloring::Always) => Some("-use-color=1"),
            Some(Coloring::Never) => Some("-use-color=0"),
        }
    }

    fn generate_report(
        self,
        cx: &Context,
        object_files: &[OsString],
        ignore_filename_regex: Option<&String>,
    ) -> Result<()> {
        let mut cmd = cx.process(&cx.llvm_cov);

        cmd.args(self.llvm_cov_args());
        cmd.args(self.use_color(cx));
        cmd.arg(format!("-instr-profile={}", cx.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));
        if let Some(jobs) = cx.jobs {
            cmd.arg(format!("-num-threads={}", jobs));
        }
        if let Some(ignore_filename_regex) = ignore_filename_regex {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename_regex);
        }

        match self {
            Self::Text | Self::Html => {
                cmd.args([
                    &format!("-show-instantiations={}", !cx.hide_instantiations),
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    &format!("-Xdemangler={}", cx.env.current_exe.display()),
                    "-Xdemangler=llvm-cov",
                    "-Xdemangler=demangle",
                ]);
                if let Some(output_dir) = &cx.output_dir {
                    if self == Self::Html {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("html")));
                    } else {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("text")));
                    }
                }
            }
            Self::Json | Self::LCov => {
                if cx.summary_only {
                    cmd.arg("-summary-only");
                }
            }
            Self::None => {}
        }

        if let Some(flags) = &cx.env.cargo_llvm_cov_flags {
            cmd.args(flags.split(' ').filter(|s| !s.trim().is_empty()));
        }

        if let Some(output_path) = &cx.output_path {
            if cx.verbose {
                status!("Running", "{}", cmd);
            }
            let out = cmd.read()?;
            fs::write(output_path, out)?;
            status!("Finished", "report saved to {}", output_path);
            return Ok(());
        }

        if cx.verbose {
            status!("Running", "{}", cmd);
        }
        cmd.run()?;
        if matches!(self, Self::Html | Self::Text) {
            if let Some(output_dir) = &cx.output_dir {
                if self == Self::Html {
                    status!("Finished", "report saved to {}", output_dir.join("html"));
                } else {
                    status!("Finished", "report saved to {}", output_dir.join("text"));
                }
            }
        }
        Ok(())
    }
}

fn ignore_filename_regex(cx: &Context) -> Option<String> {
    #[cfg(not(windows))]
    const SEPARATOR: &str = "/";
    #[cfg(windows)]
    const SEPARATOR: &str = "\\\\";

    fn default_ignore_filename_regex() -> String {
        // TODO: Should we use the actual target path instead of using `tests|examples|benches`?
        //       We may have a directory like tests/support, so maybe we need both?
        format!(
            r"(^|{0})(rustc{0}[0-9a-f]+|.cargo{0}(registry|git)|.rustup{0}toolchains|tests|examples|benches|target{0}llvm-cov-target){0}",
            SEPARATOR
        )
    }

    #[derive(Default)]
    struct Out(String);

    impl Out {
        fn push(&mut self, s: impl AsRef<str>) {
            if !self.0.is_empty() {
                self.0.push('|');
            }
            self.0.push_str(s.as_ref());
        }
    }

    let mut out = Out::default();

    if cx.disable_default_ignore_filename_regex {
        if let Some(ignore_filename) = &cx.ignore_filename_regex {
            out.push(ignore_filename);
        }
    } else {
        out.push(default_ignore_filename_regex());
        if let Some(ignore) = &cx.ignore_filename_regex {
            out.push(ignore);
        }
        if let Some(home) = home::home_dir() {
            out.push(format!("^{}{}", home.display(), SEPARATOR));
        }

        for path in resolve_excluded_paths(cx) {
            #[cfg(not(windows))]
            out.push(format!("^{}", path));
            #[cfg(windows)]
            out.push(format!("^{}", path.as_str().replace('\\', SEPARATOR)));
        }
    }

    if out.0.is_empty() {
        None
    } else {
        Some(out.0)
    }
}

fn resolve_excluded_paths(cx: &Context) -> Vec<Utf8PathBuf> {
    let excluded: Vec<_> = cx
        .workspace_members
        .excluded
        .iter()
        .map(|id| cx.metadata[id].manifest_path.parent().unwrap())
        .collect();
    let included = cx
        .workspace_members
        .included
        .iter()
        .map(|id| cx.metadata[id].manifest_path.parent().unwrap());
    let mut excluded_path = vec![];
    let mut contains: HashMap<&Utf8Path, Vec<_>> = HashMap::new();
    for included in included {
        for &excluded in excluded.iter().filter(|e| included.starts_with(e)) {
            if let Some(v) = contains.get_mut(&excluded) {
                v.push(included);
            } else {
                contains.insert(excluded, vec![included]);
            }
        }
    }
    if contains.is_empty() {
        for &manifest_dir in &excluded {
            let package_path =
                manifest_dir.strip_prefix(&cx.metadata.workspace_root).unwrap_or(manifest_dir);
            excluded_path.push(package_path.into());
        }
        return excluded_path;
    }

    for &excluded in &excluded {
        let included = match contains.get(&excluded) {
            Some(included) => included,
            None => {
                let package_path =
                    excluded.strip_prefix(&cx.metadata.workspace_root).unwrap_or(excluded);
                excluded_path.push(package_path.into());
                continue;
            }
        };

        for _ in WalkDir::new(excluded).into_iter().filter_entry(|e| {
            let p = e.path();
            if !p.is_dir() {
                if p.extension().map_or(false, |e| e == "rs") {
                    let p = p.strip_prefix(&cx.metadata.workspace_root).unwrap_or(p);
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
            let p = p.strip_prefix(&cx.metadata.workspace_root).unwrap_or(p);
            excluded_path.push(p.to_owned().try_into().unwrap());
            false
        }) {}
    }
    excluded_path
}
