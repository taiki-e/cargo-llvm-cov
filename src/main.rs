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
mod clean;
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
    slice,
};

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Clap;
use cli::RunOptions;
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Opts, Subcommand},
    config::StringOrArray,
    context::Context,
    process::ProcessBuilder,
};

fn main() {
    if let Err(e) = try_main() {
        error!("{:#}", e);
        std::process::exit(1)
    }
}

fn try_main() -> Result<()> {
    let Opts::LlvmCov(mut args) = Opts::parse();

    match args.subcommand {
        Some(Subcommand::Demangle) => {
            demangler::run()?;
        }

        Some(Subcommand::Clean(options)) => {
            clean::run(options)?;
        }

        Some(Subcommand::Run(mut args)) => {
            term::set_quiet(args.quiet);

            let cx = &Context::new(
                args.build(),
                args.manifest(),
                args.cov(),
                false,
                &[],
                args.package.as_ref().map(slice::from_ref).unwrap_or_default(),
                args.quiet,
                false,
                false,
            )?;

            clean::clean_partial(cx)?;
            create_dirs(cx)?;

            run_run(cx, &args)?;

            if !cx.cov.no_report {
                generate_report(cx)?;
            }
        }

        None => {
            term::set_quiet(args.quiet);
            if args.doctests {
                warn!("--doctests option is unstable");
            }
            if args.doc {
                args.doctests = true;
                warn!("--doc option is unstable");
            }

            let cx = &Context::new(
                args.build(),
                args.manifest(),
                args.cov(),
                args.workspace,
                &args.exclude,
                &args.package,
                args.quiet,
                args.doctests,
                args.no_run,
            )?;

            clean::clean_partial(cx)?;
            create_dirs(cx)?;
            match (args.no_run, cx.cov.no_report) {
                (false, false) => {
                    run_test(cx, &args)?;
                    generate_report(cx)?;
                }
                (false, true) => {
                    run_test(cx, &args)?;
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

fn create_dirs(cx: &Context) -> Result<()> {
    fs::create_dir_all(&cx.ws.target_dir)?;

    if let Some(output_dir) = &cx.cov.output_dir {
        fs::create_dir_all(output_dir)?;
        if cx.cov.html {
            fs::create_dir_all(output_dir.join("html"))?;
        }
        if cx.cov.text {
            fs::create_dir_all(output_dir.join("text"))?;
        }
    }

    if cx.doctests {
        fs::create_dir_all(&cx.ws.doctests_dir)?;
    }
    Ok(())
}

fn set_env(cx: &Context, cmd: &mut ProcessBuilder) {
    let llvm_profile_file = cx.ws.target_dir.join(format!("{}-%m.profraw", cx.ws.package_name));

    let rustflags = &mut cx.ws.config.rustflags().unwrap_or_default();
    rustflags.push_str(" -Z instrument-coverage");
    if !cx.cov.no_cfg_coverage {
        rustflags.push_str(" --cfg coverage");
    }
    // --remap-path-prefix is needed because sometimes macros are displayed with absolute path
    rustflags.push_str(&format!(" --remap-path-prefix {}/=", cx.ws.metadata.workspace_root));
    if cx.build.target.is_none() {
        rustflags.push_str(" --cfg trybuild_no_target");
    }

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    let rustdocflags = &mut cx.ws.config.rustdocflags();
    if cx.doctests {
        let rustdocflags = rustdocflags.get_or_insert_with(String::new);
        rustdocflags.push_str(&format!(
            " -Z instrument-coverage -Z unstable-options --persist-doctests {}",
            cx.ws.doctests_dir
        ));
        if !cx.cov.no_cfg_coverage {
            rustdocflags.push_str(" --cfg coverage");
        }
    }

    cmd.env("RUSTFLAGS", &rustflags);
    if let Some(rustdocflags) = rustdocflags {
        cmd.env("RUSTDOCFLAGS", &rustdocflags);
    }
    cmd.env("LLVM_PROFILE_FILE", &*llvm_profile_file);
    cmd.env("CARGO_INCREMENTAL", "0");
    cmd.env("CARGO_TARGET_DIR", &cx.ws.target_dir);
}

fn run_test(cx: &Context, args: &Args) -> Result<()> {
    let mut cargo = cx.cargo_process();

    set_env(cx, &mut cargo);

    cargo.arg("test");
    if cx.doctests && !args.unstable_flags.iter().any(|f| f == "doctest-in-workspace") {
        // https://github.com/rust-lang/cargo/issues/9427
        cargo.arg("-Z");
        cargo.arg("doctest-in-workspace");
    }
    cargo::test_args(cx, args, &mut cargo);

    if cx.verbose {
        status!("Running", "{}", cargo);
    }
    cargo.stdout_to_stderr().run()?;
    Ok(())
}

fn run_run(cx: &Context, args: &RunOptions) -> Result<()> {
    let mut cargo = cx.cargo_process();

    set_env(cx, &mut cargo);

    cargo.arg("run");
    cargo::run_args(cx, args, &mut cargo);

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

    if cx.cov.open {
        let path = &cx.cov.output_dir.as_ref().unwrap().join("html/index.html");
        status!("Opening", "{}", path);
        open_report(cx, path)?;
    }
    Ok(())
}

fn open_report(cx: &Context, path: &Utf8Path) -> Result<()> {
    let browser = cx.ws.config.doc.browser.as_ref().and_then(StringOrArray::path_and_args);

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
            glob::glob(
                cx.ws.target_dir.join(format!("{}-*.profraw", cx.ws.package_name)).as_str(),
            )?
            .filter_map(Result::ok),
        )
        .arg("-o")
        .arg(&cx.ws.profdata_file);
    if let Some(mode) = &cx.cov.failure_mode {
        cmd.arg(format!("-failure-mode={}", mode));
    }
    if let Some(jobs) = cx.build.jobs {
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
        WalkDir::new(target_dir)
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
    let mut target_dir = cx.ws.target_dir.clone();
    // https://doc.rust-lang.org/nightly/cargo/guide/build-cache.html
    if let Some(target) = &cx.build.target {
        target_dir.push(target);
    }
    target_dir.push(if cx.build.release { "release" } else { "debug" });
    for f in walk_target_dir(&target_dir) {
        let f = f.path();
        if is_executable::is_executable(&f) {
            files.push(f.to_owned().into_os_string());
        }
    }
    if cx.doctests {
        for f in glob::glob(cx.ws.doctests_dir.join("*/rust_out").as_str())?.filter_map(Result::ok)
        {
            if is_executable::is_executable(&f) {
                files.push(f.into_os_string());
            }
        }
    }

    // trybuild
    let trybuild_dir = &cx.ws.target_dir.join("tests");
    let mut trybuild_target = trybuild_dir.join("target");
    if let Some(target) = &cx.build.target {
        trybuild_target.push(target);
    }
    // Currently, trybuild always use debug build.
    trybuild_target.push("debug");
    if trybuild_target.is_dir() {
        let mut trybuild_targets = vec![];
        for metadata in trybuild_metadata(&cx.ws.target_dir)? {
            for package in metadata.packages {
                for target in package.targets {
                    trybuild_targets.push(target.name);
                }
            }
        }
        if !trybuild_targets.is_empty() {
            let re =
                Regex::new(&format!("^({})(-[0-9a-f]+)?$", trybuild_targets.join("|"))).unwrap();
            for entry in walk_target_dir(&trybuild_target) {
                let path = entry.path();
                if let Some(file_stem) = fs::file_stem_recursive(path).unwrap().to_str() {
                    if re.is_match(file_stem) {
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
fn trybuild_metadata(target_dir: &Utf8Path) -> Result<Vec<cargo_metadata::Metadata>> {
    let trybuild_dir = &target_dir.join("tests");
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
    fn from_args(cx: &Context) -> Vec<Self> {
        if cx.cov.json {
            vec![Self::Json]
        } else if cx.cov.lcov {
            vec![Self::LCov]
        } else if cx.cov.text {
            vec![Self::Text]
        } else if cx.cov.html {
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
        if self == Self::Text && cx.cov.output_dir.is_some() {
            return Some("-use-color=0");
        }
        match cx.build.color {
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
        cmd.arg(format!("-instr-profile={}", cx.ws.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));
        if let Some(jobs) = cx.build.jobs {
            cmd.arg(format!("-num-threads={}", jobs));
        }
        if let Some(ignore_filename_regex) = ignore_filename_regex {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename_regex);
        }

        match self {
            Self::Text | Self::Html => {
                cmd.args([
                    &format!("-show-instantiations={}", !cx.cov.hide_instantiations),
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    &format!("-Xdemangler={}", cx.env.current_exe.display()),
                    "-Xdemangler=llvm-cov",
                    "-Xdemangler=demangle",
                ]);
                if let Some(output_dir) = &cx.cov.output_dir {
                    if self == Self::Html {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("html")));
                    } else {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("text")));
                    }
                }
            }
            Self::Json | Self::LCov => {
                if cx.cov.summary_only {
                    cmd.arg("-summary-only");
                }
            }
            Self::None => {}
        }

        if let Some(flags) = &cx.env.cargo_llvm_cov_flags {
            cmd.args(flags.split(' ').filter(|s| !s.trim().is_empty()));
        }

        if let Some(output_path) = &cx.cov.output_path {
            if cx.verbose {
                status!("Running", "{}", cmd);
            }
            let out = cmd.read()?;
            fs::write(output_path, out)?;
            eprintln!();
            status!("Finished", "report saved to {}", output_path);
            return Ok(());
        }

        if cx.verbose {
            status!("Running", "{}", cmd);
        }
        cmd.run()?;
        if matches!(self, Self::Html | Self::Text) {
            if let Some(output_dir) = &cx.cov.output_dir {
                eprintln!();
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

    if cx.cov.disable_default_ignore_filename_regex {
        if let Some(ignore_filename) = &cx.cov.ignore_filename_regex {
            out.push(ignore_filename);
        }
    } else {
        out.push(default_ignore_filename_regex());
        if let Some(ignore) = &cx.cov.ignore_filename_regex {
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
        .map(|id| cx.ws.metadata[id].manifest_path.parent().unwrap())
        .collect();
    let included = cx
        .workspace_members
        .included
        .iter()
        .map(|id| cx.ws.metadata[id].manifest_path.parent().unwrap());
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
                manifest_dir.strip_prefix(&cx.ws.metadata.workspace_root).unwrap_or(manifest_dir);
            excluded_path.push(package_path.join("").into());
        }
        return excluded_path;
    }

    for &excluded in &excluded {
        let included = match contains.get(&excluded) {
            Some(included) => included,
            None => {
                let package_path =
                    excluded.strip_prefix(&cx.ws.metadata.workspace_root).unwrap_or(excluded);
                excluded_path.push(package_path.join("").into());
                continue;
            }
        };

        for _ in WalkDir::new(excluded).into_iter().filter_entry(|e| {
            let p = e.path();
            if !p.is_dir() {
                if p.extension().map_or(false, |e| e == "rs") {
                    let p = p.strip_prefix(&cx.ws.metadata.workspace_root).unwrap_or(p);
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
            let p = p.strip_prefix(&cx.ws.metadata.workspace_root).unwrap_or(p);
            excluded_path.push(p.join("").try_into().unwrap());
            false
        }) {}
    }
    excluded_path
}
