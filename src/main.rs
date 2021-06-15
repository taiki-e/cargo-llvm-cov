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
mod context;
mod demangler;
mod fs;

use std::{
    env,
    ffi::{OsStr, OsString},
    path::Path,
};

use anyhow::Result;
use serde::Deserialize;
use structopt::StructOpt;
use tracing::warn;
use walkdir::WalkDir;

use crate::{
    cli::{Args, Coloring, Opts, Subcommand},
    context::Context,
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

        let mut cargo = cx.process(&*cx.cargo);
        if !cx.cargo.nightly {
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
        out.push(DEFAULT_IGNORE_FILENAME_REGEX);
        if let Some(ignore) = &cx.ignore_filename_regex {
            out.push(ignore);
        }
        if let Some(home) = dirs_next::home_dir() {
            out.push(format!("{}/", home.display()));
        }
    }

    for path in &cx.excluded_path {
        out.push(path.as_str());
    }

    if out.0.is_empty() { None } else { Some(out.0) }
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
