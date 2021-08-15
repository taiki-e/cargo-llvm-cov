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
mod context;
mod demangler;
mod env;
mod fs;

use std::ffi::{OsStr, OsString};

use anyhow::{Context as _, Result};
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Subcommand},
    context::Context,
};

fn main() {
    if let Err(e) = try_main() {
        error!("{:#}", e);
        std::process::exit(1)
    }
}

fn try_main() -> Result<()> {
    let args = cli::from_args()?;
    if let Some(Subcommand::Demangle) = &args.subcommand {
        demangler::run()?;
        return Ok(());
    }

    let cx = &Context::new(args)?;

    match (cx.no_run, cx.no_report) {
        (false, false) => {
            clean_partial(cx)?;
            create_dirs(cx)?;
            run_test(cx)?;
            generate_report(cx)?;
        }
        (false, true) => {
            create_dirs(cx)?;
            run_test(cx)?;
        }
        (true, false) => {
            create_dirs(cx)?;
            generate_report(cx)?;
        }
        (true, true) => unreachable!(),
    }

    Ok(())
}

fn clean_partial(cx: &Context) -> Result<()> {
    if let Some(output_dir) = &cx.output_dir {
        if cx.html {
            fs::remove_dir_all(output_dir.join("html"))?;
        }
        if cx.text {
            fs::remove_dir_all(output_dir.join("text"))?;
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
    // --remap-path-prefix is needed because sometimes macros are displayed with absolute path
    rustflags.push(format!(
        " -Z instrument-coverage --remap-path-prefix {}/=",
        cx.metadata.workspace_root
    ));
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
    cargo::append_args(cx, &mut cargo);

    if cx.verbose {
        status!("Running", "{:#}", cargo);
    }
    cargo.stdout_to_stderr().run()?;
    Ok(())
}

fn generate_report(cx: &Context) -> Result<()> {
    let object_files = object_files(cx).context("failed to collect object files")?;

    merge_profraw(cx).context("failed to merge profile data")?;

    for format in Format::from_args(cx) {
        format.generate_report(cx, &object_files).context("failed to generate report")?;
    }

    if cx.open {
        open::that(cx.output_dir.as_ref().unwrap().join("html/index.html"))
            .context("couldn't open report")?;
    }
    Ok(())
}

fn merge_profraw(cx: &Context) -> Result<()> {
    // Convert raw profile data.
    let mut cmd = cx.process(&cx.llvm_profdata);
    cmd.args(&["merge", "-sparse"])
        .args(
            glob::glob(cx.target_dir.join(format!("{}-*.profraw", cx.package_name)).as_str())?
                .filter_map(Result::ok),
        )
        .arg("-o")
        .arg(&cx.profdata_file);
    if let Some(flags) = &cx.env.cargo_llvm_profdata_flags {
        cmd.args(flags.split(' ').filter(|s| !s.trim().is_empty()));
    }
    if cx.verbose {
        status!("Running", "{:#}", cmd);
    }
    cmd.run()?;
    Ok(())
}

fn object_files(cx: &Context) -> Result<Vec<OsString>> {
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

    fn generate_report(self, cx: &Context, object_files: &[OsString]) -> Result<()> {
        let mut cmd = cx.process(&cx.llvm_cov);

        cmd.args(self.llvm_cov_args());
        cmd.args(self.use_color(cx));
        cmd.arg(format!("-instr-profile={}", cx.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));

        if let Some(ignore_filename) = ignore_filename_regex(cx) {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename);
        }

        match self {
            Format::Text | Format::Html => {
                cmd.args(&[
                    &format!("-show-instantiations={}", !cx.hide_instantiations),
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    &format!("-Xdemangler={}", cx.env.current_exe.display()),
                    "-Xdemangler=llvm-cov",
                    "-Xdemangler=demangle",
                ]);
                if let Some(output_dir) = &cx.output_dir {
                    if self == Format::Html {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("html")));
                    } else {
                        cmd.arg(&format!("-output-dir={}", output_dir.join("text")));
                    }
                }
            }
            Format::Json | Format::LCov => {
                if cx.summary_only {
                    cmd.arg("-summary-only");
                }
            }
            Format::None => {}
        }

        if let Some(flags) = &cx.env.cargo_llvm_cov_flags {
            cmd.args(flags.split(' ').filter(|s| !s.trim().is_empty()));
        }

        if let Some(output_path) = &cx.output_path {
            if cx.verbose {
                status!("Running", "{:#}", cmd);
            }
            let out = cmd.read()?;
            fs::write(output_path, out)?;
            status!("Finished", "report saved to {:#}", output_path);
            return Ok(());
        }

        if cx.verbose {
            status!("Running", "{:#}", cmd);
        }
        cmd.run()?;
        if matches!(self, Self::Html | Self::Text) {
            if let Some(output_dir) = &cx.output_dir {
                if self == Self::Html {
                    status!("Finished", "report saved to {:#}", output_dir.join("html"));
                } else {
                    status!("Finished", "report saved to {:#}", output_dir.join("text"));
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
        if let Some(home) = dirs_next::home_dir() {
            out.push(format!("^{}{}", home.display(), SEPARATOR));
        }

        for path in &cx.excluded_path {
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
