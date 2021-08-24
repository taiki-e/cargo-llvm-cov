use camino::Utf8PathBuf;
use clap::{AppSettings, ArgSettings, Clap};
use serde::Deserialize;

const ABOUT: &str =
    "Cargo subcommand to easily use LLVM source-based code coverage (-Z instrument-coverage).

Use -h for short descriptions and --help for more details.";

const MAX_TERM_WIDTH: usize = 100;

#[derive(Debug, Clap)]
#[clap(
    bin_name = "cargo",
    version,
    max_term_width(MAX_TERM_WIDTH),
    setting(AppSettings::DeriveDisplayOrder),
    setting(AppSettings::StrictUtf8),
    setting(AppSettings::UnifiedHelpMessage)
)]
pub(crate) enum Opts {
    #[clap(about(ABOUT), version)]
    LlvmCov(Args),
}

#[derive(Debug, Clap)]
#[clap(
    bin_name = "cargo llvm-cov",
    about(ABOUT),
    max_term_width(MAX_TERM_WIDTH),
    setting(AppSettings::DeriveDisplayOrder),
    setting(AppSettings::StrictUtf8),
    setting(AppSettings::UnifiedHelpMessage)
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
    #[clap(long, conflicts_with = "json", conflicts_with = "lcov")]
    pub(crate) text: bool,
    /// Generate coverage reports in "html" format.
    ////
    /// If --output-dir is not specified, the report will be generated in `target/llvm-cov/html` directory.
    ///
    /// This internally calls `llvm-cov show -format=html`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[clap(long, conflicts_with = "json", conflicts_with = "lcov", conflicts_with = "text")]
    pub(crate) html: bool,
    /// Generate coverage reports in "html" format and open them in a browser after the operation.
    ///
    /// See --html for more.
    #[clap(long, conflicts_with = "json", conflicts_with = "lcov", conflicts_with = "text")]
    pub(crate) open: bool,

    /// Export only summary information for each file in the coverage data.
    ///
    /// This flag can only be used together with either --json or --lcov.
    // If the format flag is not specified, this flag is no-op because the only summary is displayed anyway.
    #[clap(long, conflicts_with = "text", conflicts_with = "html", conflicts_with = "open")]
    pub(crate) summary_only: bool,
    /// Specify a file to write coverage data into.
    ///
    /// This flag can only be used together with --json, --lcov, or --text.
    /// See --output-dir for --html and --open.
    #[clap(
        long,
        value_name = "PATH",
        conflicts_with = "html",
        conflicts_with = "open",
        setting(ArgSettings::ForbidEmptyValues)
    )]
    pub(crate) output_path: Option<Utf8PathBuf>,
    /// Specify a directory to write coverage reports into (default to `target/llvm-cov`).
    ///
    /// This flag can only be used together with --text, --html, or --open.
    /// See also --output-path.
    // If the format flag is not specified, this flag is no-op.
    #[clap(
        long,
        value_name = "DIRECTORY",
        conflicts_with = "json",
        conflicts_with = "lcov",
        conflicts_with = "output-path",
        setting(ArgSettings::ForbidEmptyValues)
    )]
    pub(crate) output_dir: Option<Utf8PathBuf>,

    /// Skip source code files with file paths that match the given regular expression.
    #[clap(long, value_name = "PATTERN", setting(ArgSettings::ForbidEmptyValues))]
    pub(crate) ignore_filename_regex: Option<String>,
    // For debugging (unstable)
    #[clap(long, hidden = true)]
    pub(crate) disable_default_ignore_filename_regex: bool,
    // For debugging (unstable)
    #[clap(long, hidden = true)]
    pub(crate) hide_instantiations: bool,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    #[clap(long)]
    pub(crate) doctests: bool,
    /// Run tests, but don't generate coverage reports
    #[clap(long, conflicts_with = "no-run")]
    pub(crate) no_report: bool,

    // =========================================================================
    // `cargo test` options
    // https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
    /// Generate coverage reports without running tests
    #[clap(long)]
    pub(crate) no_run: bool,
    /// Run all tests regardless of failure
    #[clap(long)]
    pub(crate) no_fail_fast: bool,
    /// Package to run tests for
    // cargo allows the combination of --package and --workspace, but we reject
    // it because the situation where both flags are specified is odd.
    #[clap(
        short,
        long,
        multiple_occurrences = true,
        value_name = "SPEC",
        conflicts_with = "workspace",
        setting(ArgSettings::ForbidEmptyValues)
    )]
    pub(crate) package: Vec<String>,
    /// Test all packages in the workspace
    #[clap(long, visible_alias = "all")]
    pub(crate) workspace: bool,
    /// Exclude packages from the test
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "SPEC",
        requires = "workspace",
        setting(ArgSettings::ForbidEmptyValues)
    )]
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
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "FEATURES",
        setting(ArgSettings::ForbidEmptyValues)
    )]
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
    #[clap(long, value_name = "TRIPLE", setting(ArgSettings::ForbidEmptyValues))]
    pub(crate) target: Option<String>,
    // TODO: Currently, we are using a subdirectory of the target directory as
    //       the actual target directory. What effect should this option have
    //       on its behavior?
    // /// Directory for all generated artifacts
    // #[clap(long, value_name = "DIRECTORY")]
    // target_dir: Option<Utf8PathBuf>,
    /// Path to Cargo.toml
    #[clap(long, value_name = "PATH", setting(ArgSettings::ForbidEmptyValues))]
    pub(crate) manifest_path: Option<Utf8PathBuf>,
    /// Use verbose output (-vv/-vvv propagate verbosity to cargo)
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
    #[clap(
        short = 'Z',
        multiple_occurrences = true,
        value_name = "FLAG",
        setting(ArgSettings::ForbidEmptyValues)
    )]
    pub(crate) unstable_flags: Vec<String>,

    /// Arguments for the test binary
    #[clap(last = true, setting(ArgSettings::ForbidEmptyValues))]
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
        max_term_width = MAX_TERM_WIDTH,
        setting = AppSettings::DeriveDisplayOrder,
        setting = AppSettings::StrictUtf8,
        setting = AppSettings::UnifiedHelpMessage,
        setting = AppSettings::Hidden,
    )]
    Demangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, clap::ArgEnum)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub(crate) enum Coloring {
    Auto = 0,
    Always,
    Never,
}

impl Coloring {
    pub(crate) fn cargo_color(self) -> &'static str {
        clap::ArgEnum::as_arg(&self).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{env, panic, path::Path, process::Command};

    use anyhow::Result;
    use clap::{Clap, IntoApp};
    use fs_err as fs;
    use tempfile::Builder;

    use super::{Args, Opts, MAX_TERM_WIDTH};

    // https://github.com/clap-rs/clap/issues/751
    #[cfg(unix)]
    #[test]
    fn non_utf8_arg() {
        use std::{ffi::OsStr, os::unix::prelude::OsStrExt};

        // `cargo llvm-cov -- $'fo\x80o'`
        Opts::try_parse_from(&[
            "cargo".as_ref(),
            "llvm-cov".as_ref(),
            "--".as_ref(),
            OsStr::from_bytes(&[b'f', b'o', 0x80, b'o']),
        ])
        .unwrap_err();
    }

    // https://github.com/clap-rs/clap/issues/1772
    #[test]
    fn multiple_occurrences() {
        let Opts::LlvmCov(args) =
            Opts::try_parse_from(&["cargo", "llvm-cov", "--features", "a", "--features", "b"])
                .unwrap();
        assert_eq!(args.features, ["a", "b"]);

        let Opts::LlvmCov(args) =
            Opts::try_parse_from(&["cargo", "llvm-cov", "--package", "a", "--package", "b"])
                .unwrap();
        assert_eq!(args.package, ["a", "b"]);

        let Opts::LlvmCov(args) = Opts::try_parse_from(&[
            "cargo",
            "llvm-cov",
            "--exclude",
            "a",
            "--exclude",
            "b",
            "--all",
        ])
        .unwrap();
        assert_eq!(args.exclude, ["a", "b"]);

        let Opts::LlvmCov(args) =
            Opts::try_parse_from(&["cargo", "llvm-cov", "-Z", "a", "-Zb"]).unwrap();
        assert_eq!(args.unstable_flags, ["a", "b"]);

        let Opts::LlvmCov(args) =
            Opts::try_parse_from(&["cargo", "llvm-cov", "--", "a", "b"]).unwrap();
        assert_eq!(args.args, ["a", "b"]);
    }

    // https://github.com/clap-rs/clap/issues/1740
    #[test]
    fn empty_value() {
        Opts::try_parse_from(&["cargo", "llvm-cov", "--output-path", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--output-dir", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--ignore-filename-regex", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--package", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--exclude", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--features", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--target", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--manifest-path", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "-Z", ""]).unwrap_err();
        Opts::try_parse_from(&["cargo", "llvm-cov", "--", ""]).unwrap_err();
    }

    fn get_help(long: bool) -> Result<String> {
        let mut buf = vec![];
        if long {
            Args::into_app().term_width(MAX_TERM_WIDTH).write_long_help(&mut buf)?;
        } else {
            Args::into_app().term_width(MAX_TERM_WIDTH).write_help(&mut buf)?;
        }
        let mut out = String::new();
        for mut line in String::from_utf8(buf)?.lines() {
            if let Some(new) = line.trim_end().strip_suffix(env!("CARGO_PKG_VERSION")) {
                line = new;
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
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
        let actual = get_help(true).unwrap();
        assert_diff("tests/long-help.txt", actual);
    }

    #[test]
    fn short_help() {
        let actual = get_help(false).unwrap();
        assert_diff("tests/short-help.txt", actual);
    }

    #[test]
    fn update_readme() -> Result<()> {
        let new = get_help(true)?;
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
