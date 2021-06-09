use std::{ffi::OsString, path::PathBuf, str::FromStr};

use anyhow::{bail, Error, Result};
use structopt::{clap::AppSettings, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(
    bin_name = "cargo",
    rename_all = "kebab-case",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
pub(crate) enum Opts {
    /// A wrapper for source based code coverage (-Zinstrument-coverage).
    ///
    /// Use -h for short descriptions and --help for more details.
    LlvmCov(Args),
}

#[derive(Debug, StructOpt)]
#[structopt(
    rename_all = "kebab-case",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
pub(crate) struct Args {
    /// Export coverage data in "json" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[structopt(long)]
    pub(crate) json: bool,
    /// Export coverage data in "lcov" format.
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=lcov`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[structopt(long, conflicts_with = "json")]
    pub(crate) lcov: bool,

    /// Generate coverage reports in “text” format.
    ///
    /// If --output-path or --output-dir is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov show -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[structopt(long, conflicts_with_all = &["json", "lcov"])]
    pub(crate) text: bool,
    /// Generate coverage reports in "html" format.
    ////
    /// If --output-dir is not specified, the report will be generated in `target/llvm-cov` directory.
    ///
    /// This internally calls `llvm-cov show -format=html`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[structopt(long, conflicts_with_all = &["json", "lcov", "text"])]
    pub(crate) html: bool,
    /// Generate coverage reports in "html" format and open them in a browser after the operation.
    ///
    /// See --html for more.
    #[structopt(long, conflicts_with_all = &["json", "lcov", "text"])]
    pub(crate) open: bool,

    /// Export only summary information for each file in the coverage data.
    ///
    /// This flag can only be used together with either --json or --lcov.
    // If the format flag is not specified, this flag is no-op because the only summary is displayed anyway.
    #[structopt(long, conflicts_with_all = &["text", "html", "open"])]
    pub(crate) summary_only: bool,
    /// Specify a file to write coverage data into.
    ///
    /// This flag can only be used together with --json, --lcov, or --text.
    /// See --output-dir for --html and --open.
    #[structopt(long, value_name = "PATH", conflicts_with_all = &["html", "open"])]
    pub(crate) output_path: Option<PathBuf>,
    /// Specify a directory to write coverage reports into (default to `target/llvm-cov`).
    ///
    /// This flag can only be used together with --text, --html, or --open.
    /// See also --output-path.
    // If the format flag is not specified, this flag is no-op.
    #[structopt(long, value_name = "DIRECTORY", conflicts_with_all = &["json", "lcov", "output-path"])]
    pub(crate) output_dir: Option<PathBuf>,

    /// Skip source code files with file paths that match the given regular expression.
    #[structopt(long, value_name = "PATTERN")]
    pub(crate) ignore_filename_regex: Option<String>,
    // For debugging (unstable)
    #[structopt(long, hidden = true)]
    pub(crate) disable_default_ignore_filename_regex: bool,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    #[structopt(long)]
    pub(crate) doctests: bool,

    // =========================================================================
    // `cargo test` options
    // https://doc.rust-lang.org/cargo/commands/cargo-test.html
    /// Compile, but don't run tests (unstable)
    #[structopt(long, hidden = true)]
    pub(crate) no_run: bool,
    /// Run all tests regardless of failure
    #[structopt(long)]
    pub(crate) no_fail_fast: bool,
    // TODO: --package doesn't work properly, use --manifest-path instead for now.
    // /// Package to run tests for
    // #[structopt(short, long, value_name = "SPEC")]
    // package: Vec<String>,
    /// Test all packages in the workspace
    #[structopt(long, visible_alias = "all")]
    pub(crate) workspace: bool,
    /// Exclude packages from the test
    #[structopt(long, value_name = "SPEC")]
    pub(crate) exclude: Vec<String>,
    // TODO: Should this only work for cargo's --jobs? Or should it also work
    //       for llvm-cov's -num-threads?
    // /// Number of parallel jobs, defaults to # of CPUs
    // #[structopt(short, long, value_name = "N")]
    // jobs: Option<u64>,
    /// Build artifacts in release mode, with optimizations
    #[structopt(long)]
    pub(crate) release: bool,
    /// Space or comma separated list of features to activate
    #[structopt(long, value_name = "FEATURES")]
    pub(crate) features: Vec<String>,
    /// Activate all available features
    #[structopt(long)]
    pub(crate) all_features: bool,
    /// Do not activate the `default` feature
    #[structopt(long)]
    pub(crate) no_default_features: bool,
    /// Build for the target triple
    #[structopt(long, value_name = "TRIPLE")]
    pub(crate) target: Option<String>,
    // TODO: Currently, we are using a subdirectory of the target directory as
    //       the actual target directory. What effect should this option have
    //       on its behavior?
    // /// Directory for all generated artifacts
    // #[structopt(long, value_name = "DIRECTORY", parse(from_os_str))]
    // target_dir: Option<PathBuf>,
    /// Path to Cargo.toml
    #[structopt(long, value_name = "PATH", parse(from_os_str))]
    pub(crate) manifest_path: Option<PathBuf>,
    /// Use verbose output (-vv very verbose/build.rs output)
    #[structopt(short, long, parse(from_occurrences))]
    pub(crate) verbose: u8,
    /// Coloring: auto, always, never
    // This flag will be propagated to both cargo and llvm-cov.
    #[structopt(long, value_name = "WHEN")]
    pub(crate) color: Option<Coloring>,
    /// Require Cargo.lock and cache are up to date
    #[structopt(long)]
    pub(crate) frozen: bool,
    /// Require Cargo.lock is up to date
    #[structopt(long)]
    pub(crate) locked: bool,

    /// Unstable (nightly-only) flags to Cargo
    #[structopt(short = "Z", value_name = "FLAG")]
    pub(crate) unstable_flags: Vec<String>,

    /// Arguments for the test binary
    #[structopt(last = true, parse(from_os_str))]
    pub(crate) args: Vec<OsString>,
}

impl Args {
    pub(crate) fn show(&self) -> bool {
        self.text || self.html
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Coloring {
    Auto,
    Always,
    Never,
}

impl Coloring {
    pub(crate) fn cargo_color(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl FromStr for Coloring {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => bail!("must be auto, always, or never, but found `{}`", other),
        }
    }
}
