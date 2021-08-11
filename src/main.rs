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
mod term;
#[macro_use]
mod process;

mod cargo;
mod cli;
mod context;
mod demangler;
mod fs;

use std::ffi::{OsStr, OsString};

use anyhow::{Context as _, Result};
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Subcommand},
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
    trace::init();

    let args = cli::from_args()?;
    if let Some(Subcommand::Demangle) = &args.subcommand {
        demangler::run()?;
        return Ok(());
    }

    let cx = &Context::new(args)?;

    if !cx.no_run {
        clean_partial(cx)?;
        run_test(cx)?;
    }
    generate_report(cx)?;

    Ok(())
}

fn clean_partial(cx: &Context) -> Result<()> {
    debug!("cleaning build artifacts");

    if let Some(output_dir) = &cx.output_dir {
        fs::remove_dir_all(output_dir)?;
        fs::create_dir_all(output_dir)?;
    }

    for path in glob::glob(cx.target_dir.join("*.profraw").as_str())?.filter_map(Result::ok) {
        fs::remove_file(path)?;
    }

    if cx.doctests {
        fs::remove_dir_all(&cx.doctests_dir)?;
        fs::create_dir_all(&cx.doctests_dir)?;
    }

    fs::remove_file(&cx.profdata_file)?;
    Ok(())
}

fn run_test(cx: &Context) -> Result<()> {
    debug!("running tests");

    let llvm_profile_file = cx.target_dir.join(format!("{}-%m.profraw", cx.package_name));

    let rustflags = &mut cx.env.rustflags.clone().unwrap_or_default();
    // --remap-path-prefix is needed because sometimes macros are displayed with absolute path
    rustflags.push(format!(
        " -Z instrument-coverage --remap-path-prefix {}/=",
        cx.metadata.workspace_root
    ));

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
    cargo.env("LLVM_PROFILE_FILE", &*llvm_profile_file);
    cargo.env("CARGO_INCREMENTAL", "0");
    if let Some(rustdocflags) = rustdocflags {
        cargo.env("RUSTDOCFLAGS", &rustdocflags);
    }

    cargo.args(&["test", "--target-dir"]).arg(&cx.target_dir);
    if cx.doctests && !cx.unstable_flags.iter().any(|f| f == "doctest-in-workspace") {
        // https://github.com/rust-lang/cargo/issues/9427
        cargo.arg("-Z");
        cargo.arg("doctest-in-workspace");
    }
    append_args(cx, &mut cargo);

    cargo.stdout_to_stderr().run()?;
    Ok(())
}

fn generate_report(cx: &Context) -> Result<()> {
    debug!("generating reports");

    let object_files = object_files(cx).context("failed to collect object files")?;

    merge_profraw(cx).context("failed to merge profile data")?;

    for format in Format::from_args(cx) {
        format.generate_report(cx, &object_files).context("failed to generate report")?;
    }

    if cx.open {
        open::that(cx.output_dir.as_ref().unwrap().join("index.html"))
            .context("couldn't open report")?;
    }
    Ok(())
}

fn merge_profraw(cx: &Context) -> Result<()> {
    debug!("merging profile data");

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
    debug!("collecting profile data");

    let mut files = vec![];
    // To support testing binary crate like tests that use the CARGO_BIN_EXE
    // environment variable, pass all compiled executables.
    // This is not the ideal way, but the way unstable book says it is cannot support them.
    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#tips-for-listing-the-binaries-automatically
    let mut target_dir = cx.target_dir.clone();
    if let Some(target) = &cx.target {
        target_dir.push(target);
    }
    target_dir.push(if cx.release { "release" } else { "debug" });
    fs::remove_dir_all(target_dir.join("incremental"))?;
    for f in WalkDir::new(target_dir.as_str()).into_iter().filter_map(Result::ok) {
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
    let trybuild_dir = &cx.metadata.target_directory.join("tests");
    let mut trybuild_target = trybuild_dir.join("target");
    if let Some(target) = &cx.target {
        trybuild_target.push(target);
    }
    // Currently, trybuild always use debug build.
    trybuild_target.push("debug");
    fs::remove_dir_all(trybuild_target.join("incremental"))?;
    if trybuild_target.is_dir() {
        let mut trybuild_projects = vec![];
        for entry in fs::read_dir(trybuild_dir)?.filter_map(Result::ok) {
            let manifest_path = entry.path().join("Cargo.toml");
            if !manifest_path.is_file() {
                continue;
            }
            for package in cargo_metadata::MetadataCommand::new()
                .manifest_path(manifest_path)
                .no_deps()
                .exec()?
                .packages
            {
                for target in package.targets {
                    trybuild_projects.push(target.name);
                }
            }
        }
        if !trybuild_projects.is_empty() {
            let re = Regex::new(&format!("^({})-[0-9a-f]+$", trybuild_projects.join("|"))).unwrap();
            for entry in WalkDir::new(trybuild_target).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if let Some(path) = path.file_name().unwrap().to_str() {
                    if re.is_match(path) {
                        // Excludes dummy binaries generated by trybuild.
                        // https://github.com/dtolnay/trybuild/blob/54ddc67c9e2f236d44ac6640a2327e178ff6ae68/src/run.rs#L228-L231
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
    fn from_args(args: &Args) -> Vec<Self> {
        if args.json {
            vec![Self::Json]
        } else if args.lcov {
            vec![Self::LCov]
        } else if args.text {
            vec![Self::Text]
        } else if args.html {
            vec![Self::Html, Self::None]
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

    fn generate_report(self, cx: &Context, object_files: &[OsString]) -> Result<()> {
        debug!("generating report for format {:?}", self);

        let mut cmd = cx.process(&cx.llvm_cov);

        cmd.args(self.llvm_cov_args());
        cmd.args(self.use_color(cx.color));
        cmd.arg(format!("-instr-profile={}", cx.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));

        if let Some(ignore_filename) = ignore_filename_regex(cx) {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename);
        }

        match self {
            Format::Text | Format::Html => {
                cmd.args(&[
                    "-show-instantiations",
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    &format!("-Xdemangler={}", cx.env.current_exe.display()),
                    "-Xdemangler=llvm-cov",
                    "-Xdemangler=demangle",
                ]);
                if let Some(output_dir) = &cx.output_dir {
                    cmd.arg(&format!("-output-dir={}", output_dir));
                }
            }
            Format::Json | Format::LCov => {
                if cx.summary_only {
                    cmd.arg("-summary-only");
                }
            }
            Format::None => {}
        }

        if let Some(output_path) = &cx.output_path {
            let out = cmd.read()?;
            fs::write(output_path, out)?;
            return Ok(());
        }

        cmd.run()?;
        Ok(())
    }
}

fn ignore_filename_regex(cx: &Context) -> Option<String> {
    #[cfg(not(windows))]
    const SEPARATOR: &str = "/";
    #[cfg(windows)]
    const SEPARATOR: &str = "\\\\";

    fn default_ignore_filename_regex() -> String {
        format!(
            r"rustc{0}|.cargo{0}(registry|git){0}|.rustup{0}toolchains{0}|tests{0}|examples{0}|benches{0}|target{0}llvm-cov-target{0}",
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
        if let Some(home) = dirs_next::home_dir() {
            out.push(format!("{}{}", home.display(), SEPARATOR));
        }
    }

    for path in &cx.excluded_path {
        #[cfg(not(windows))]
        out.push(path.as_str());
        #[cfg(windows)]
        out.push(path.as_str().replace('\\', SEPARATOR));
    }

    if out.0.is_empty() {
        None
    } else {
        Some(out.0)
    }
}

fn append_args(cx: &Context, cmd: &mut ProcessBuilder) {
    if cx.no_fail_fast {
        cmd.arg("--no-fail-fast");
    }
    for package in &cx.package {
        cmd.arg("--package");
        cmd.arg(package);
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
