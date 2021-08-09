use std::{env, ffi::OsString};

use anyhow::{format_err, Result};
use camino::Utf8PathBuf;
use clap::{AppSettings, Clap};
use serde::Deserialize;

// Clap panics if you pass a non-utf8 value to an argument that expects a utf8
// value.
//
// clap 3.0.0-beta.2:
//      thread 'main' panicked at 'unexpected invalid UTF-8 code point', $CARGO/clap-3.0.0-beta.2/src/parse/matches/arg_matches.rs:220:28
//
// clap 2.33.3:
//      thread 'main' panicked at 'unexpected invalid UTF-8 code point', $CARGO/clap-2.33.3/src/args/arg_matches.rs:217:28
//
// Even if you store a value as OsString and pass to cargo as is, you will get
// the same panic on cargo side. e.g., `cargo check --manifest-path $'fo\x80o'`
fn handle_args(args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<Vec<String>> {
    // Adapted from https://github.com/rust-lang/rust/blob/3bc9dd0dd293ab82945e35888ed6d7ab802761ef/compiler/rustc_driver/src/lib.rs#L1365-L1375.
    args.into_iter()
        .enumerate()
        .map(|(i, arg)| {
            arg.into()
                .into_string()
                .map_err(|arg| format_err!("argument {} is not valid Unicode: {:?}", i, arg))
        })
        .collect()
}

pub(crate) fn from_args() -> Result<Args> {
    let Opts::LlvmCov(args) = Opts::parse_from(handle_args(env::args_os())?);
    Ok(args)
}

#[derive(Debug, Clap)]
#[clap(
    bin_name = "cargo",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
enum Opts {
    /// Wrapper for source based code coverage (-Z instrument-coverage).
    ///
    /// Use -h for short descriptions and --help for more details.
    LlvmCov(Args),
}

/// Wrapper for source based code coverage (-Z instrument-coverage).
///
/// Use -h for short descriptions and --help for more details.
#[derive(Debug, Clap)]
#[clap(
    bin_name = "cargo llvm-cov",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
pub(crate) struct Args {
    #[clap(subcommand)]
    pub(crate) subcommand: Option<Subcommand>,

    /// Export coverage data in "json" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[clap(long)]
    pub(crate) json: bool,
    /// Export coverage data in "lcov" format.
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=lcov`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[clap(long, conflicts_with = "json")]
    pub(crate) lcov: bool,

    /// Generate coverage reports in “text” format.
    ///
    /// If --output-path or --output-dir is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov show -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[clap(long, conflicts_with_all = &["json", "lcov"])]
    pub(crate) text: bool,
    /// Generate coverage reports in "html" format.
    ////
    /// If --output-dir is not specified, the report will be generated in `target/llvm-cov` directory.
    ///
    /// This internally calls `llvm-cov show -format=html`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[clap(long, conflicts_with_all = &["json", "lcov", "text"])]
    pub(crate) html: bool,
    /// Generate coverage reports in "html" format and open them in a browser after the operation.
    ///
    /// See --html for more.
    #[clap(long, conflicts_with_all = &["json", "lcov", "text"])]
    pub(crate) open: bool,

    /// Export only summary information for each file in the coverage data.
    ///
    /// This flag can only be used together with either --json or --lcov.
    // If the format flag is not specified, this flag is no-op because the only summary is displayed anyway.
    #[clap(long, conflicts_with_all = &["text", "html", "open"])]
    pub(crate) summary_only: bool,
    /// Specify a file to write coverage data into.
    ///
    /// This flag can only be used together with --json, --lcov, or --text.
    /// See --output-dir for --html and --open.
    #[clap(long, value_name = "PATH", conflicts_with_all = &["html", "open"])]
    pub(crate) output_path: Option<Utf8PathBuf>,
    /// Specify a directory to write coverage reports into (default to `target/llvm-cov`).
    ///
    /// This flag can only be used together with --text, --html, or --open.
    /// See also --output-path.
    // If the format flag is not specified, this flag is no-op.
    #[clap(long, value_name = "DIRECTORY", conflicts_with_all = &["json", "lcov", "output-path"])]
    pub(crate) output_dir: Option<Utf8PathBuf>,

    /// Skip source code files with file paths that match the given regular expression.
    #[clap(long, value_name = "PATTERN")]
    pub(crate) ignore_filename_regex: Option<String>,
    // For debugging (unstable)
    #[clap(long, hidden = true)]
    pub(crate) disable_default_ignore_filename_regex: bool,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    #[clap(long)]
    pub(crate) doctests: bool,

    // =========================================================================
    // `cargo test` options
    // https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
    /// Compile, but don't run tests (unstable)
    #[clap(long)]
    pub(crate) no_run: bool,
    /// Run all tests regardless of failure
    #[clap(long)]
    pub(crate) no_fail_fast: bool,
    /// Package to run tests for
    #[clap(short, long, value_name = "SPEC")]
    pub(crate) package: Vec<String>,
    /// Test all packages in the workspace
    #[clap(long, visible_alias = "all")]
    pub(crate) workspace: bool,
    /// Exclude packages from the test
    #[clap(long, value_name = "SPEC", requires = "workspace")]
    pub(crate) exclude: Vec<String>,
    // TODO: Should this only work for cargo's --jobs? Or should it also work
    //       for llvm-cov's -num-threads?
    // /// Number of parallel jobs, defaults to # of CPUs
    // #[clap(short, long, value_name = "N")]
    // jobs: Option<u64>,
    /// Build artifacts in release mode, with optimizations
    #[clap(long)]
    pub(crate) release: bool,
    /// Space or comma separated list of features to activate
    #[clap(long, value_name = "FEATURES")]
    pub(crate) features: Vec<String>,
    /// Activate all available features
    #[clap(long)]
    pub(crate) all_features: bool,
    /// Do not activate the `default` feature
    #[clap(long)]
    pub(crate) no_default_features: bool,
    /// Build for the target triple
    ///
    /// When this option is used, coverage for proc-macro and build script will
    /// not be displayed because cargo does not pass RUSTFLAGS to them.
    #[clap(long, value_name = "TRIPLE")]
    pub(crate) target: Option<String>,
    // TODO: Currently, we are using a subdirectory of the target directory as
    //       the actual target directory. What effect should this option have
    //       on its behavior?
    // /// Directory for all generated artifacts
    // #[clap(long, value_name = "DIRECTORY")]
    // target_dir: Option<Utf8PathBuf>,
    /// Path to Cargo.toml
    #[clap(long, value_name = "PATH")]
    pub(crate) manifest_path: Option<Utf8PathBuf>,
    /// Use verbose output (-vv very verbose/build.rs output)
    #[clap(short, long, parse(from_occurrences))]
    pub(crate) verbose: u8,
    /// Coloring
    // This flag will be propagated to both cargo and llvm-cov.
    #[clap(long, arg_enum, value_name = "WHEN")]
    pub(crate) color: Option<Coloring>,
    /// Require Cargo.lock and cache are up to date
    #[clap(long)]
    pub(crate) frozen: bool,
    /// Require Cargo.lock is up to date
    #[clap(long)]
    pub(crate) locked: bool,

    /// Unstable (nightly-only) flags to Cargo
    #[clap(short = 'Z', value_name = "FLAG")]
    pub(crate) unstable_flags: Vec<String>,

    /// Arguments for the test binary
    #[clap(last = true)]
    pub(crate) args: Vec<String>,
}

impl Args {
    pub(crate) fn show(&self) -> bool {
        self.text || self.html
    }
}

#[derive(Debug, Clap)]
pub(crate) enum Subcommand {
    // internal (unstable)
    #[clap(
        setting = AppSettings::DeriveDisplayOrder,
        setting = AppSettings::UnifiedHelpMessage,
        setting = AppSettings::Hidden,
    )]
    Demangle,
}

#[derive(Debug, Clone, Copy, Deserialize, clap::ArgEnum)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Coloring {
    Auto,
    Always,
    Never,
}

impl Coloring {
    // TODO: use clap::ArgEnum::as_arg instead once new version of clap released.
    pub(crate) fn cargo_color(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, panic, path::Path, process::Command};

    use anyhow::Result;
    use clap::IntoApp;
    use tempfile::Builder;

    use super::Args;
    use crate::fs;

    // See handle_args function for more.
    #[cfg(unix)]
    #[test]
    fn non_utf8_arg() {
        use std::{ffi::OsStr, os::unix::prelude::OsStrExt};

        use clap::Clap;

        use super::Opts;

        // `cargo llvm-cov -- $'fo\x80o'`
        let res = panic::catch_unwind(|| {
            drop(Opts::try_parse_from(&[
                "cargo".as_ref(),
                "llvm-cov".as_ref(),
                "--".as_ref(),
                OsStr::from_bytes(&[b'f', b'o', 0x80, b'o']),
            ]));
        });
        assert!(res.is_err());

        super::handle_args(&[
            "cargo".as_ref(),
            "llvm-cov".as_ref(),
            "--".as_ref(),
            OsStr::from_bytes(&[b'f', b'o', 0x80, b'o']),
        ])
        .unwrap_err();
    }

    fn get_long_help() -> Result<String> {
        let mut buf = vec![];
        Args::into_app().term_width(80).write_long_help(&mut buf)?;
        let mut out = String::new();
        for mut line in String::from_utf8(buf)?.lines() {
            if let Some(new) = line.trim_end().strip_suffix(env!("CARGO_PKG_VERSION")) {
                line = new;
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out.pop();
        Ok(out)
    }

    #[track_caller]
    fn assert_diff(expected_path: impl AsRef<Path>, actual: impl AsRef<str>) {
        let actual = actual.as_ref();
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let expected_path = &manifest_dir.join(expected_path);
        if !expected_path.is_file() {
            fs::write(expected_path, "").unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        if expected != actual {
            if env::var_os("CI").is_some() {
                let outdir = Builder::new().prefix("assert_diff").tempdir().unwrap();
                let actual_path = &outdir.path().join(expected_path.file_name().unwrap());
                fs::write(actual_path, actual).unwrap();
                let status = Command::new("git")
                    .args(&["--no-pager", "diff", "--no-index", "--"])
                    .args(&[expected_path, actual_path])
                    .status()
                    .unwrap();
                assert!(!status.success());
                panic!("assertion failed");
            } else {
                fs::write(expected_path, actual).unwrap();
            }
        }
    }

    #[test]
    fn long_help() {
        let actual = get_long_help().unwrap();
        assert_diff("tests/long-help.txt", actual);
    }

    #[test]
    fn update_readme() -> Result<()> {
        let new = get_long_help()?;
        let path = &Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
        let base = fs::read_to_string(path)?;
        let mut out = String::with_capacity(base.capacity());
        let mut lines = base.lines();
        let mut start = false;
        let mut end = false;
        while let Some(line) = lines.next() {
            out.push_str(line);
            out.push('\n');
            if line == "<!-- readme-long-help:start -->" {
                start = true;
                out.push_str("```console\n");
                out.push_str("$ cargo llvm-cov --help\n");
                out.push_str(&new);
                for line in &mut lines {
                    if line == "<!-- readme-long-help:end -->" {
                        out.push_str("```\n");
                        out.push_str(line);
                        out.push('\n');
                        end = true;
                        break;
                    }
                }
            }
        }
        if start && end {
            fs::write(path, out)?;
        } else if start {
            panic!("missing `<!-- readme-long-help:end -->` comment in README.md");
        } else {
            panic!("missing `<!-- readme-long-help:start -->` comment in README.md");
        }
        Ok(())
    }
}
