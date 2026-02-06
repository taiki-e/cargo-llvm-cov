// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{ffi::OsString, io, mem, str::FromStr};

use anyhow::{Error, Result, bail, format_err};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_config2::Color;
use lexopt::{
    Arg::{Long, Short, Value},
    ValueExt as _,
};

use crate::{env, process::ProcessBuilder, term};

// TODO: add --config option and passthrough to cargo-config: https://github.com/rust-lang/cargo/pull/10755/

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) subcommand: Subcommand,

    // -------------------------------------------------------------------------
    // Operation-specific options
    /// Options only referred in "build"-related operations. (subcommands building/testing/running crates and show-env subcommand)
    pub(crate) build: BuildOptions,
    /// Options only referred in "report" operations. (report subcommand and subcommands reporting coverage)
    pub(crate) report: ReportOptions,
    /// Options only referred in "clean" operations. (clean subcommand and subcommands building rust code)
    pub(crate) clean: CleanOptions,
    /// Options only referred in "show-env" operations. (show-env subcommand)
    pub(crate) show_env: ShowEnvOptions,

    // -------------------------------------------------------------------------
    // Options referred by various operations
    /// Including doc tests (unstable)
    ///
    /// This flag is unstable.
    /// See <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for more.
    pub(crate) doctests: bool,

    // /// Generate coverage report without running tests
    // pub(crate) no_run: bool,
    /// Test all packages in the workspace
    pub(crate) workspace: bool,

    // /// Number of parallel jobs, defaults to # of CPUs
    // // i32 or string "default": https://github.com/rust-lang/cargo/blob/0.80.0/src/cargo/core/compiler/build_config.rs#L84-L97
    // pub(crate) jobs: Option<i32>,
    /// Build artifacts in release mode, with optimizations
    pub(crate) release: bool,
    /// Build artifacts with the specified profile
    ///
    /// On `cargo llvm-cov nextest`/`cargo llvm-cov nextest-archive` this is the
    /// value of `--cargo-profile` option, otherwise this is the value of  `--profile` option.
    pub(crate) cargo_profile: Option<String>,
    // /// Space or comma separated list of features to activate
    // pub(crate) features: Vec<String>,
    // /// Activate all available features
    // pub(crate) all_features: bool,
    // /// Do not activate the `default` feature
    // pub(crate) no_default_features: bool,
    /// Build for the target triple
    pub(crate) target: Option<String>,
    // TODO: Currently, we are using a subdirectory of the target directory as
    //       the actual target directory. What effect should this option have
    //       on its behavior?
    // /// Directory for all generated artifacts
    // target_dir: Option<Utf8PathBuf>,
    /// Use verbose output
    ///
    /// Use -vv (-vvv) to propagate verbosity to cargo.
    pub(crate) verbose: u8,

    /// Use --remap-path-prefix for workspace root
    ///
    /// Note that this does not fully compatible with doctest.
    pub(crate) remap_path_prefix: bool,

    /// Show coverage of the specified dependency instead of the crates in the current workspace.
    pub(crate) dep_coverage: Vec<String>,

    pub(crate) nextest_archive_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Subcommand {
    /// Run tests and generate coverage report.
    None,

    /// Run tests and generate coverage report.
    Test,

    /// Run a binary or example and generate coverage report.
    Run,

    /// Generate coverage report.
    Report { nextest_archive_file: bool },

    /// Remove artifacts that cargo-llvm-cov has generated in the past
    Clean,

    /// Output the environment set by cargo-llvm-cov to build Rust projects.
    ShowEnv,

    /// Run tests with cargo nextest
    Nextest { archive_file: bool },

    /// Build and archive tests with cargo nextest
    NextestArchive,
}

static CARGO_LLVM_COV_USAGE: &str = include_str!("../docs/cargo-llvm-cov.txt");
static CARGO_LLVM_COV_TEST_USAGE: &str = include_str!("../docs/cargo-llvm-cov-test.txt");
static CARGO_LLVM_COV_RUN_USAGE: &str = include_str!("../docs/cargo-llvm-cov-run.txt");
static CARGO_LLVM_COV_REPORT_USAGE: &str = include_str!("../docs/cargo-llvm-cov-report.txt");
static CARGO_LLVM_COV_CLEAN_USAGE: &str = include_str!("../docs/cargo-llvm-cov-clean.txt");
static CARGO_LLVM_COV_SHOW_ENV_USAGE: &str = include_str!("../docs/cargo-llvm-cov-show-env.txt");
static CARGO_LLVM_COV_NEXTEST_USAGE: &str = include_str!("../docs/cargo-llvm-cov-nextest.txt");
static CARGO_LLVM_COV_NEXTEST_ARCHIVE_USAGE: &str =
    include_str!("../docs/cargo-llvm-cov-nextest-archive.txt");

impl Subcommand {
    fn can_passthrough(subcommand: Self) -> bool {
        matches!(subcommand, Self::Test | Self::Run | Self::Nextest { .. } | Self::NextestArchive)
    }

    fn help_text(subcommand: Self) -> &'static str {
        match subcommand {
            Self::None => CARGO_LLVM_COV_USAGE,
            Self::Test => CARGO_LLVM_COV_TEST_USAGE,
            Self::Run => CARGO_LLVM_COV_RUN_USAGE,
            Self::Report { .. } => CARGO_LLVM_COV_REPORT_USAGE,
            Self::Clean => CARGO_LLVM_COV_CLEAN_USAGE,
            Self::ShowEnv => CARGO_LLVM_COV_SHOW_ENV_USAGE,
            Self::Nextest { .. } => CARGO_LLVM_COV_NEXTEST_USAGE,
            Self::NextestArchive => CARGO_LLVM_COV_NEXTEST_ARCHIVE_USAGE,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Test => "test",
            Self::Run => "run",
            Self::Report { .. } => "report",
            Self::Clean => "clean",
            Self::ShowEnv => "show-env",
            Self::Nextest { .. } => "nextest",
            Self::NextestArchive => "nextest-archive",
        }
    }

    pub(crate) fn call_cargo_nextest(self) -> bool {
        matches!(self, Self::Nextest { .. } | Self::NextestArchive)
    }
    pub(crate) fn read_nextest_archive(self) -> bool {
        matches!(
            self,
            Self::Nextest { archive_file: true } | Self::Report { nextest_archive_file: true }
        )
    }
}

impl FromStr for Subcommand {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "test" | "t" => Ok(Self::Test),
            "run" | "r" => Ok(Self::Run),
            "report" => Ok(Self::Report { nextest_archive_file: false }),
            "clean" => Ok(Self::Clean),
            "show-env" => Ok(Self::ShowEnv),
            "nextest" => Ok(Self::Nextest { archive_file: false }),
            "nextest-archive" => Ok(Self::NextestArchive),
            _ => bail!("unrecognized subcommand {s}"),
        }
    }
}

/// Options only referred in "build"-related operations. (subcommands building/testing/running crates and show-env subcommand)
#[derive(Debug, Default)]
pub(crate) struct BuildOptions {
    /// Run all tests regardless of failure and generate report
    ///
    /// If tests failed but report generation succeeded, exit with a status of 0.
    pub(crate) ignore_run_fail: bool,
    /// Any of --lib, --bin, --bins, --example, --examples, --test, --tests, --bench, --benches, --all-targets, or --doc.
    pub(crate) has_target_selection_options: bool,
    /// Packages additional excluded from the test (--exclude-from-test)
    ///
    /// (--exclude is already contained in `cargo_args` field.)
    pub(crate) exclude_from_test: Vec<String>,

    /// Enable branch coverage. (unstable)
    pub(crate) branch: bool,
    /// Enable mcdc coverage. (unstable)
    pub(crate) mcdc: bool,

    /// Include coverage of C/C++ code linked to Rust library/binary
    ///
    /// Note that `CC`/`CXX`/`LLVM_COV`/`LLVM_PROFDATA` environment variables
    /// must be set to Clang/LLVM compatible with the LLVM version used in rustc.
    // TODO: support specifying languages like: --include-ffi=c,  --include-ffi=c,c++
    pub(crate) include_ffi: bool,
    /// Activate coverage reporting only for the target triple
    ///
    /// Activate coverage reporting only for the target triple specified via `--target`.
    /// This is important, if the project uses multiple targets via the cargo
    /// bindeps feature, and not all targets can use `instrument-coverage`,
    /// e.g. a microkernel, or an embedded binary.
    ///
    /// When this flag is used, coverage for proc-macro and build script will not be displayed.
    pub(crate) coverage_target_only: bool,
    /// Unset cfg(coverage), which is enabled when code is built using cargo-llvm-cov.
    pub(crate) no_cfg_coverage: bool,
    /// Unset cfg(coverage_nightly), which is enabled when code is built using cargo-llvm-cov and nightly compiler.
    pub(crate) no_cfg_coverage_nightly: bool,

    /// Build without setting RUSTC_WRAPPER
    ///
    /// By default, cargo-llvm-cov sets RUSTC_WRAPPER. This is usually optimal
    /// for compilation time, execution time, and disk usage.
    ///
    /// When both this flag and --target option are used, coverage for proc-macro and
    /// build script will not be displayed because cargo does not pass RUSTFLAGS to them.
    pub(crate) no_rustc_wrapper: bool,

    pub(crate) cargo_args: Vec<String>,
    /// Arguments for the test binary
    pub(crate) rest: Vec<String>,
}

/// Options only referred in "report" operations. (report subcommand and subcommands reporting coverage)
#[derive(Debug, Default)]
pub(crate) struct ReportOptions {
    /// Run tests, but don't generate coverage report
    pub(crate) no_report: bool,

    /// Export coverage data in "json" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    pub(crate) json: bool,
    /// Export coverage data in "lcov" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=lcov`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    pub(crate) lcov: bool,

    /// Export coverage data in "cobertura" XML format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=lcov` and then converts to cobertura.xml.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    pub(crate) cobertura: bool,

    /// Export coverage data in "Codecov Custom Coverage" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=json` and then converts to codecov.json.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    pub(crate) codecov: bool,

    /// Generate coverage report in "text" format
    ///
    /// If --output-path or --output-dir is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov show -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    pub(crate) text: bool,
    /// Generate coverage report in "html" format
    ///
    /// If --output-dir is not specified, the report will be generated in `target/llvm-cov/html` directory.
    ///
    /// This internally calls `llvm-cov show -format=html`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    pub(crate) html: bool,
    /// Generate coverage reports in "html" format and open them in a browser after the operation.
    ///
    /// See --html for more.
    pub(crate) open: bool,

    /// Export only summary information for each file in the coverage data
    ///
    /// This flag can only be used together with --json, --lcov, --cobertura, or --codecov.
    // If the format flag is not specified, this flag is no-op because the only summary is displayed anyway.
    pub(crate) summary_only: bool,

    /// Specify a file to write coverage data into.
    ///
    /// This flag can only be used together with --json, --lcov, --cobertura, --codecov, or --text.
    /// See --output-dir for --html and --open.
    pub(crate) output_path: Option<Utf8PathBuf>,
    /// Specify a directory to write coverage report into (default to `target/llvm-cov`).
    ///
    /// This flag can only be used together with --text, --html, or --open.
    /// See also --output-path.
    // If the format flag is not specified, this flag is no-op.
    pub(crate) output_dir: Option<Utf8PathBuf>,

    /// Fail if `any` or `all` profiles cannot be merged (default to `any`)
    pub(crate) failure_mode: Option<String>,
    /// Skip source code files with file paths that match the given regular expression.
    pub(crate) ignore_filename_regex: Option<String>,
    // For debugging (unstable)
    pub(crate) no_default_ignore_filename_regex: bool,
    /// Show instantiations in report
    pub(crate) show_instantiations: bool,
    /// Exit with a status of 1 if the total function coverage is less than MIN percent.
    pub(crate) fail_under_functions: Option<f64>,
    /// Exit with a status of 1 if the total line coverage is less than MIN percent.
    pub(crate) fail_under_lines: Option<f64>,
    /// Exit with a status of 1 if the total region coverage is less than MIN percent.
    pub(crate) fail_under_regions: Option<f64>,
    /// Exit with a status of 1 if the uncovered lines are greater than MAX.
    pub(crate) fail_uncovered_lines: Option<u64>,
    /// Exit with a status of 1 if the uncovered regions are greater than MAX.
    pub(crate) fail_uncovered_regions: Option<u64>,
    /// Exit with a status of 1 if the uncovered functions are greater than MAX.
    pub(crate) fail_uncovered_functions: Option<u64>,
    /// Show lines with no coverage.
    pub(crate) show_missing_lines: bool,
    /// Include build script in coverage report.
    pub(crate) include_build_script: bool,
    /// Skip functions in coverage report.
    pub(crate) skip_functions: bool,
}

impl ReportOptions {
    pub(crate) const fn show(&self) -> bool {
        self.text || self.html
    }

    fn validate(&self, subcommand: Subcommand) -> Result<()> {
        // Handle options specific to certain subcommands.
        let (subcommands_without_report, no_report_incompat) = match subcommand {
            // subcommands without generate_report in main.rs.
            Subcommand::Clean | Subcommand::ShowEnv | Subcommand::NextestArchive => (true, true),
            Subcommand::Report { .. } => (false, true),
            Subcommand::None | Subcommand::Test | Subcommand::Run | Subcommand::Nextest { .. } => {
                (false, false)
            }
        };
        if no_report_incompat && self.no_report {
            if matches!(subcommand, Subcommand::NextestArchive) {
                specific_flag_warn("--no-report", subcommand, &["test", "run", "nextest", ""]);
            } else {
                specific_flag("--no-report", subcommand, &["test", "run", "nextest", ""])?;
            }
        }
        if subcommands_without_report || self.no_report {
            let Self {
                no_report: _,
                json,
                lcov,
                cobertura,
                codecov,
                text,
                html,
                open,
                summary_only,
                output_path,
                output_dir,
                failure_mode,
                ignore_filename_regex,
                no_default_ignore_filename_regex,
                show_instantiations,
                fail_under_functions,
                fail_under_lines,
                fail_under_regions,
                fail_uncovered_lines,
                fail_uncovered_regions,
                fail_uncovered_functions,
                show_missing_lines,
                include_build_script,
                skip_functions,
            } = self;
            for (flag, passed) in [
                ("--json", *json),
                ("--lcov", *lcov),
                ("--cobertura", *cobertura),
                ("--codecov", *codecov),
                ("--text", *text),
                ("--html", *html),
                ("--open", *open),
                ("--summary-only", *summary_only),
                ("--output-path", output_path.is_some()),
                ("--output-dir", output_dir.is_some()),
                ("--failure-mode", failure_mode.is_some()),
                ("--ignore-filename-regex", ignore_filename_regex.is_some()),
                ("--no-default-ignore-filename-regex", *no_default_ignore_filename_regex),
                ("--show-instantiations", *show_instantiations),
                ("--fail-under-functions", fail_under_functions.is_some()),
                ("--fail-under-lines", fail_under_lines.is_some()),
                ("--fail-under-regions", fail_under_regions.is_some()),
                ("--fail-uncovered-lines", fail_uncovered_lines.is_some()),
                ("--fail-uncovered-regions", fail_uncovered_regions.is_some()),
                ("--fail-uncovered-functions", fail_uncovered_functions.is_some()),
                ("--show-missing-lines", *show_missing_lines),
                ("--include-build-script", *include_build_script),
                ("--skip-functions", *skip_functions),
            ] {
                if passed {
                    if subcommands_without_report {
                        specific_flag_warn(flag, subcommand, &[
                            "report", "test", "run", "nextest", "",
                        ]);
                    } else {
                        conflicts_warn(flag, "--no-report");
                    }
                }
            }
        }

        // conflicts
        // TODO: handle these mutual exclusions elegantly.
        if self.lcov {
            let flag = "--lcov";
            if self.json {
                conflicts(flag, "--json")?;
            }
        }
        if self.cobertura {
            let flag = "--cobertura";
            if self.json {
                conflicts(flag, "--json")?;
            }
            if self.lcov {
                conflicts(flag, "--lcov")?;
            }
            if self.codecov {
                conflicts(flag, "--codecov")?;
            }
        }
        if self.codecov {
            let flag = "--codecov";
            if self.json {
                conflicts(flag, "--json")?;
            }
            if self.lcov {
                conflicts(flag, "--lcov")?;
            }
            if self.cobertura {
                conflicts(flag, "--cobertura")?;
            }
        }
        if self.text {
            let flag = "--text";
            if self.json {
                conflicts(flag, "--json")?;
            }
            if self.lcov {
                conflicts(flag, "--lcov")?;
            }
            if self.cobertura {
                conflicts(flag, "--cobertura")?;
            }
            if self.codecov {
                conflicts(flag, "--codecov")?;
            }
        }
        if self.html || self.open {
            let flag = if self.html { "--html" } else { "--open" };
            if self.json {
                conflicts(flag, "--json")?;
            }
            if self.lcov {
                conflicts(flag, "--lcov")?;
            }
            if self.cobertura {
                conflicts(flag, "--cobertura")?;
            }
            if self.codecov {
                conflicts(flag, "--codecov")?;
            }
            if self.text {
                conflicts(flag, "--text")?;
            }
        }
        if self.summary_only || self.output_path.is_some() {
            let flag = if self.summary_only { "--summary-only" } else { "--output-path" };
            if self.html {
                conflicts(flag, "--html")?;
            }
            if self.open {
                conflicts(flag, "--open")?;
            }
        }
        if self.skip_functions {
            let flag = "--skip-functions";
            if self.html {
                conflicts(flag, "--html")?;
            }
        }
        if self.output_dir.is_some() {
            let flag = "--output-dir";
            if self.json {
                conflicts(flag, "--json")?;
            }
            if self.lcov {
                conflicts(flag, "--lcov")?;
            }
            if self.cobertura {
                conflicts(flag, "--cobertura")?;
            }
            if self.codecov {
                conflicts(flag, "--codecov")?;
            }
            if self.output_path.is_some() {
                conflicts(flag, "--output-path")?;
            }
        }

        Ok(())
    }
}

/// Options only referred in "clean" operations. (clean subcommand and subcommands building rust code)
#[derive(Debug, Default)]
pub(crate) struct CleanOptions {
    /// Build without cleaning any old build artifacts.
    ///
    /// Note that this can cause false positives/false negatives due to old build artifacts.
    pub(crate) no_clean: bool,
    /// Clean only profraw files
    pub(crate) profraw_only: bool,

    // These are for clean; others also accept them, but don't need to store
    // because these flags are handled as passthrough args.
    // https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html#manifest-options
    pub(crate) frozen: bool,
    pub(crate) locked: bool,
    pub(crate) offline: bool,
}

impl CleanOptions {
    fn validate(&self, subcommand: Subcommand) -> Result<()> {
        let (no_clean_incompat, profraw_only_incompat) = match subcommand {
            Subcommand::Report { .. } | Subcommand::ShowEnv => (true, true),
            Subcommand::Clean => (true, false),
            Subcommand::None
            | Subcommand::Test
            | Subcommand::Run
            | Subcommand::Nextest { .. }
            | Subcommand::NextestArchive => (false, true),
        };
        if no_clean_incompat && self.no_clean {
            specific_flag("--no-clean", subcommand, &[
                "test",
                "run",
                "nextest",
                "nextest-archive",
                "",
            ])?;
        }
        if profraw_only_incompat && self.profraw_only {
            specific_flag("--profraw-only", subcommand, &["clean"])?;
        }
        Ok(())
    }

    pub(crate) fn cargo_args(&self, cmd: &mut ProcessBuilder) {
        if self.frozen {
            cmd.arg("--frozen");
        }
        if self.locked {
            cmd.arg("--locked");
        }
        if self.offline {
            cmd.arg("--offline");
        }
    }
}

/// Options only referred in "show-env" operations. (show-env subcommand)
#[derive(Debug, Clone)]
pub(crate) struct ShowEnvOptions {
    pub(crate) show_env_format: ShowEnvFormat,
}

impl ShowEnvOptions {
    #[allow(clippy::fn_params_excessive_bools)]
    pub(crate) fn new(
        subcommand: Subcommand,
        sh: bool,
        pwsh: bool,
        cmd: bool,
        csh: bool,
        fish: bool,
        nu: bool,
        xonsh: bool,
    ) -> Result<Self> {
        let show_env_format = if subcommand != Subcommand::ShowEnv {
            for (flag, passed) in [
                ("--sh", sh),
                ("--pwsh", pwsh),
                ("--cmd", cmd),
                ("--csh", csh),
                ("--fish", fish),
                ("--nu", nu),
                ("--xonsh", xonsh),
            ] {
                if passed {
                    specific_flag(flag, subcommand, &["show-env"])?;
                }
            }
            ShowEnvFormat::default()
        } else if sh {
            if pwsh {
                conflicts("--sh", "--pwsh")?;
            }
            if cmd {
                conflicts("--sh", "--cmd")?;
            }
            if csh {
                conflicts("--sh", "--csh")?;
            }
            if fish {
                conflicts("--sh", "--fish")?;
            }
            if nu {
                conflicts("--sh", "--nu")?;
            }
            if xonsh {
                conflicts("--sh", "--xonsh")?;
            }
            ShowEnvFormat::Sh
        } else if pwsh {
            if cmd {
                conflicts("--pwsh", "--cmd")?;
            }
            if csh {
                conflicts("--pwsh", "--csh")?;
            }
            if fish {
                conflicts("--pwsh", "--fish")?;
            }
            if nu {
                conflicts("--pwsh", "--nu")?;
            }
            if xonsh {
                conflicts("--pwsh", "--xonsh")?;
            }
            ShowEnvFormat::Pwsh
        } else if cmd {
            if csh {
                conflicts("--cmd", "--csh")?;
            }
            if fish {
                conflicts("--cmd", "--fish")?;
            }
            if nu {
                conflicts("--cmd", "--nu")?;
            }
            if xonsh {
                conflicts("--cmd", "--xonsh")?;
            }
            ShowEnvFormat::Cmd
        } else if csh {
            if fish {
                conflicts("--csh", "--fish")?;
            }
            if nu {
                conflicts("--csh", "--nu")?;
            }
            if xonsh {
                conflicts("--csh", "--xonsh")?;
            }
            ShowEnvFormat::Csh
        } else if fish {
            if nu {
                conflicts("--fish", "--nu")?;
            }
            if xonsh {
                conflicts("--fish", "--xonsh")?;
            }
            ShowEnvFormat::Fish
        } else if nu {
            if xonsh {
                conflicts("--nu", "--xonsh")?;
            }
            ShowEnvFormat::Nu
        } else if xonsh {
            ShowEnvFormat::Xonsh
        } else {
            ShowEnvFormat::default()
        };
        Ok(Self { show_env_format })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) enum ShowEnvFormat {
    /// Each key-value: `{key}={value}`, where `{value}` is escaped using [`shell_escape::escape`].
    #[default]
    EscapedKeyValuePair,
    /// Each key-value: `export {key}={value}`, where `{value}` is escaped using [`shell_escape::unix::escape`].
    Sh,
    /// Each key-value: `$env:{key}="{value}"`, where `{value}` is PowerShell Unicode escaped e.g. "`u{72}".
    Pwsh,
    /// Each key-value: `set {key}={value}`, where `{value}` is escaped using [`shell_escape::windows::escape`].
    Cmd,
    /// Each key-value: `setenv {key} '{value}'`, where `{value}` is escaped using [`shell_escape::unix::escape`].
    Csh,
    /// Each key-value: `set -gx {key}={value}`, where `{value}` is escaped using [`shell_escape::unix::escape`].
    Fish,
    /// Each key-value: `$env.{key} = {value}`, where `{value}` is escaped using [`shell_escape::unix::escape`].
    Nu,
    /// Each key-value: `${key} = '{value}'`, where `{value}` is escaped using [`shell_escape::unix::escape`].
    Xonsh,
}

impl ShowEnvFormat {
    pub(crate) fn writeln(
        &self,
        writer: &mut dyn io::Write,
        key: &str,
        value: &str,
    ) -> io::Result<()> {
        match self {
            ShowEnvFormat::EscapedKeyValuePair => {
                writeln!(writer, "{key}={}", shell_escape::escape(value.into()))
            }
            ShowEnvFormat::Sh => {
                // TODO: https://github.com/sfackler/shell-escape/issues/6
                writeln!(writer, "export {key}={}", escape::sh(value.into()))
            }
            ShowEnvFormat::Pwsh => {
                writeln!(writer, "$env:{key}=\"{}\"", escape::pwsh(value))
            }
            ShowEnvFormat::Cmd => {
                writeln!(writer, "set {key}={}", escape::cmd(value.into()))
            }
            ShowEnvFormat::Csh => {
                // TODO: https://en.wikipedia.org/wiki/C_shell#Quoting_and_escaping
                let value = escape::sh(value.into());
                if value.starts_with('"') || value.starts_with('\'') {
                    writeln!(writer, "setenv {key} {value}")
                } else {
                    writeln!(writer, "setenv {key} '{value}'")
                }
            }
            ShowEnvFormat::Fish => {
                // TODO: https://fishshell.com/docs/current/language.html#quotes
                writeln!(writer, "set -gx {key}={}", escape::sh(value.into()))
            }
            ShowEnvFormat::Nu => {
                // TODO: https://www.nushell.sh/lang-guide/chapters/strings_and_text.html#string-quoting
                let value = escape::sh(value.into());
                if value.starts_with('"') || value.starts_with('\'') {
                    writeln!(writer, "$env.{key} = {value}")
                } else {
                    writeln!(writer, "$env.{key} = '{value}'")
                }
            }
            ShowEnvFormat::Xonsh => {
                // TODO: https://xon.sh/tutorial_subproc_strings.html
                let value = escape::sh(value.into());
                if value.starts_with('"') || value.starts_with('\'') {
                    writeln!(writer, "${key} = {value}")
                } else {
                    writeln!(writer, "${key} = '{value}'")
                }
            }
        }
    }
}

pub(crate) mod escape {
    pub(crate) use shell_escape::{unix::escape as sh, windows::escape as cmd};
    pub(crate) fn pwsh(s: &str) -> String {
        // PowerShell 6+ expects encoded UTF-8 text. Some env vars like CARGO_ENCODED_RUSTFLAGS
        // have non-printable binary characters. We can work around this and any other escape
        // related considerations by just escaping all characters. Rust's Unicode escape is
        // of form "\u{<code>}", but PowerShell expects "`u{<code>}". A replace call fixes
        // this.
        s.escape_unicode().to_string().replace('\\', "`")
    }
}

// Arguments only referred in Context::new/Workspace::new.
// It will be dropped at an early stage.
pub(crate) struct UnresolvedArgs {
    /// Package to run tests for
    pub(crate) package: Vec<String>,
    /// Packages from the report (--exclude and --exclude-from-report)
    pub(crate) exclude_from_report: Vec<String>,
    /// Path to Cargo.toml
    pub(crate) manifest_path: Option<Utf8PathBuf>,
    /// Coloring
    // This flag will be propagated to both cargo and llvm-cov.
    pub(crate) color: Option<Color>,
}

pub(crate) fn merge_config_and_args(
    ws: &mut crate::cargo::Workspace,
    target: &mut Option<String>,
    verbose: &mut u8,
    color: Option<Color>,
) -> Result<()> {
    // CLI flags are prefer over config values.
    if target.is_none() {
        target.clone_from(&ws.config.build_target_for_cli(None::<&str>)?.pop());
    }
    if *verbose == 0 {
        *verbose = ws.config.term.verbose.unwrap_or(false) as u8;
    }
    if let Some(color) = color {
        ws.config.term.color = Some(color);
    }
    Ok(())
}

pub(crate) const FIRST_SUBCMD: &str = "llvm-cov";

impl Args {
    pub(crate) fn parse() -> Result<Option<(Self, UnresolvedArgs)>> {
        // rustc/cargo args must be valid Unicode
        // https://github.com/rust-lang/rust/blob/1.84.0/compiler/rustc_driver_impl/src/args.rs#L121
        // TODO: https://github.com/rust-lang/cargo/pull/11118
        fn handle_args(
            args: impl IntoIterator<Item = impl Into<OsString>>,
        ) -> impl Iterator<Item = Result<String>> {
            args.into_iter().enumerate().map(|(i, arg)| {
                arg.into().into_string().map_err(|arg| {
                    #[allow(clippy::unnecessary_debug_formatting)]
                    {
                        format_err!("argument {} is not valid Unicode: {arg:?}", i + 1)
                    }
                })
            })
        }

        let mut raw_args = handle_args(env::args_os());
        raw_args.next(); // cargo
        match raw_args.next().transpose()? {
            Some(arg) if arg == FIRST_SUBCMD => {}
            Some(arg) => bail!("expected subcommand '{FIRST_SUBCMD}', found argument '{arg}'"),
            None => bail!("expected subcommand '{FIRST_SUBCMD}'"),
        }
        let mut args = vec![];
        for arg in &mut raw_args {
            let arg = arg?;
            if arg == "--" {
                break;
            }
            args.push(arg);
        }
        let rest = raw_args.collect::<Result<Vec<_>>>()?;

        let mut cargo_args = vec![];
        let mut subcommand = Subcommand::None;
        let mut after_subcommand = false;

        let mut manifest_path = None;
        let mut color = None;

        let mut doctests = false;
        let mut no_run = false;
        let mut no_fail_fast = false;
        let mut ignore_run_fail = false;
        let mut lib = false;
        let mut bin: Vec<String> = vec![];
        let mut bins = false;
        let mut example: Vec<String> = vec![];
        let mut examples = false;
        let mut test: Vec<String> = vec![];
        let mut tests = false;
        let mut bench: Vec<String> = vec![];
        let mut benches = false;
        let mut all_targets = false;
        let mut doc = false;

        let mut package: Vec<String> = vec![];
        let mut workspace = false;
        let mut exclude = vec![];
        let mut exclude_from_test = vec![];
        let mut exclude_from_report = vec![];

        let mut no_cfg_coverage = false;
        let mut no_cfg_coverage_nightly = false;
        let mut dep_coverage = vec![];
        let mut branch = false;
        let mut mcdc = false;

        let mut report = ReportOptions::default();
        let mut clean = CleanOptions::default();

        // build options
        let mut release = false;
        let mut target = None;
        let mut coverage_target_only = false;
        let mut remap_path_prefix = false;
        let mut include_ffi = false;
        let mut verbose: usize = 0;
        let mut no_rustc_wrapper = false;

        // show-env options
        let mut sh = false;
        let mut pwsh = false;
        let mut cmd = false;
        let mut csh = false;
        let mut fish = false;
        let mut nu = false;
        let mut xonsh = false;

        // options ambiguous between nextest-related and others
        let mut profile = None;
        let mut cargo_profile = None;
        let mut archive_file = None;
        let mut nextest_archive_file = None;

        let mut parser = lexopt::Parser::from_args(args);
        while let Some(arg) = parser.next()? {
            macro_rules! parse_opt {
                ($opt:tt $(.$field:ident)? $(,)?) => {{
                    if Store::is_full(&$opt $(.$field)?) {
                        multi_arg(&arg)?;
                    }
                    Store::push(&mut $opt $(.$field)?, &parser.value()?.into_string().unwrap())?;
                    after_subcommand = false;
                }};
            }
            macro_rules! parse_opt_passthrough {
                ($opt:tt $(.$field:ident)? $(,)?) => {{
                    if Store::is_full(&$opt $(.$field)?) {
                        multi_arg(&arg)?;
                    }
                    match arg {
                        Long(flag) => {
                            let flag = format!("--{flag}");
                            if let Some(val) = parser.optional_value() {
                                let val = val.into_string().unwrap();
                                Store::push(&mut $opt $(.$field)?, &val)?;
                                cargo_args.push(format!("{flag}={val}"));
                            } else {
                                let val = parser.value()?.into_string().unwrap();
                                Store::push(&mut $opt $(.$field)?, &val)?;
                                cargo_args.push(flag);
                                cargo_args.push(val);
                            }
                        }
                        Short(flag) => {
                            if let Some(val) = parser.optional_value() {
                                let val = val.into_string().unwrap();
                                Store::push(&mut $opt, &val)?;
                                cargo_args.push(format!("-{flag}{val}"));
                            } else {
                                let val = parser.value()?.into_string().unwrap();
                                Store::push(&mut $opt, &val)?;
                                cargo_args.push(format!("-{flag}"));
                                cargo_args.push(val);
                            }
                        }
                        Value(_) => unreachable!(),
                    }
                    after_subcommand = false;
                }};
            }
            macro_rules! parse_multi_opt {
                ($v:ident $(,)?) => {{
                    let val = parser.value()?;
                    let mut val = val.to_str().unwrap();
                    if val.starts_with('\'') && val.ends_with('\'')
                        || val.starts_with('"') && val.ends_with('"')
                    {
                        val = &val[1..val.len() - 1];
                    }
                    let sep = if val.contains(',') { ',' } else { ' ' };
                    $v.extend(val.split(sep).filter(|s| !s.is_empty()).map(str::to_owned));
                }};
            }
            macro_rules! parse_flag {
                ($flag:tt $(.$field:ident)? $(,)?) => {{
                    if mem::replace(&mut $flag $(.$field)?, true) {
                        multi_arg(&arg)?;
                    }
                    #[allow(unused_assignments)]
                    {
                        after_subcommand = false;
                    }
                }};
            }
            macro_rules! parse_flag_passthrough {
                ($flag:tt $(.$field:ident)? $(,)?) => {{
                    parse_flag!($flag $(.$field)?);
                    passthrough!();
                }};
            }
            macro_rules! passthrough {
                () => {{
                    match arg {
                        Long(flag) => {
                            let flag = format!("--{flag}");
                            if let Some(val) = parser.optional_value() {
                                cargo_args.push(format!("{flag}={}", val.string()?));
                            } else {
                                cargo_args.push(flag);
                            }
                        }
                        Short(flag) => {
                            if let Some(val) = parser.optional_value() {
                                cargo_args.push(format!("-{flag}{}", val.string()?));
                            } else {
                                cargo_args.push(format!("-{flag}"));
                            }
                        }
                        Value(_) => unreachable!(),
                    }
                    after_subcommand = false;
                }};
            }

            match arg {
                Long("color") => parse_opt_passthrough!(color),
                Long("manifest-path") => parse_opt!(manifest_path),
                Long("frozen") => parse_flag_passthrough!(clean.frozen),
                Long("locked") => parse_flag_passthrough!(clean.locked),
                Long("offline") => parse_flag_passthrough!(clean.offline),

                Long("doctests") => parse_flag!(doctests),
                Long("ignore-run-fail") => parse_flag!(ignore_run_fail),
                Long("no-run") => parse_flag!(no_run),
                Long("no-fail-fast") => parse_flag_passthrough!(no_fail_fast),

                Long("lib") => parse_flag_passthrough!(lib),
                Long("bin") => parse_opt_passthrough!(bin),
                Long("bins") => parse_flag_passthrough!(bins),
                Long("example") => parse_opt_passthrough!(example),
                Long("examples") => parse_flag_passthrough!(examples),
                Long("test") => parse_opt_passthrough!(test),
                Long("tests") => parse_flag_passthrough!(tests),
                Long("bench") => parse_opt_passthrough!(bench),
                Long("benches") => parse_flag_passthrough!(benches),
                Long("all-targets") => parse_flag_passthrough!(all_targets),
                Long("doc") => parse_flag_passthrough!(doc),

                Short('p') | Long("package") => parse_opt_passthrough!(package),
                Long("workspace" | "all") => parse_flag_passthrough!(workspace),
                Long("exclude") => parse_opt_passthrough!(exclude),
                Long("exclude-from-test") => parse_opt!(exclude_from_test),
                Long("exclude-from-report") => parse_opt!(exclude_from_report),

                Long("no-cfg-coverage") => parse_flag!(no_cfg_coverage),
                Long("no-cfg-coverage-nightly") => parse_flag!(no_cfg_coverage_nightly),
                Long("dep-coverage") => parse_multi_opt!(dep_coverage),

                // build options
                Short('r') | Long("release") => parse_flag!(release),
                // ambiguous between nextest-related and others will be handled later
                Long("profile") => parse_opt!(profile),
                Long("cargo-profile") => parse_opt!(cargo_profile),
                Long("target") => parse_opt!(target),
                Long("coverage-target-only") => parse_flag!(coverage_target_only),
                Long("remap-path-prefix") => parse_flag!(remap_path_prefix),
                Long("include-ffi") => parse_flag!(include_ffi),
                Long("no-clean") => parse_flag!(clean.no_clean),
                Long("no-rustc-wrapper") => parse_flag!(no_rustc_wrapper),

                // clean options
                Long("profraw-only") => parse_flag!(clean.profraw_only),

                // report options
                Long("no-report") => parse_flag!(report.no_report),
                Long("json") => parse_flag!(report.json),
                Long("lcov") => parse_flag!(report.lcov),
                Long("cobertura") => parse_flag!(report.cobertura),
                Long("codecov") => parse_flag!(report.codecov),
                Long("text") => parse_flag!(report.text),
                Long("html") => parse_flag!(report.html),
                Long("open") => parse_flag!(report.open),
                Long("summary-only") => parse_flag!(report.summary_only),
                Long("skip-functions") => parse_flag!(report.skip_functions),
                Long("branch") => parse_flag!(branch),
                Long("mcdc") => parse_flag!(mcdc),
                Long("output-path") => parse_opt!(report.output_path),
                Long("output-dir") => parse_opt!(report.output_dir),
                Long("failure-mode") => parse_opt!(report.failure_mode),
                Long("ignore-filename-regex") => parse_opt!(report.ignore_filename_regex),
                Long(
                    flag @ ("no-default-ignore-filename-regex"
                    | "disable-default-ignore-filename-regex"),
                ) => {
                    if flag == "disable-default-ignore-filename-regex" {
                        renamed(flag, "--no-default-ignore-filename-regex");
                    }
                    parse_flag!(report.no_default_ignore_filename_regex);
                }
                Long("show-instantiations") => parse_flag!(report.show_instantiations),
                Long("hide-instantiations") => {
                    // The following warning is a hint, so it should not be promoted to an error.
                    let _guard = term::warn::ignore();
                    warn!("--hide-instantiations is now enabled by default");
                }
                Long("fail-under-functions") => parse_opt!(report.fail_under_functions),
                Long("fail-under-lines") => parse_opt!(report.fail_under_lines),
                Long("fail-under-regions") => parse_opt!(report.fail_under_regions),
                Long("fail-uncovered-lines") => parse_opt!(report.fail_uncovered_lines),
                Long("fail-uncovered-regions") => parse_opt!(report.fail_uncovered_regions),
                Long("fail-uncovered-functions") => parse_opt!(report.fail_uncovered_functions),
                Long("show-missing-lines") => parse_flag!(report.show_missing_lines),
                Long("include-build-script") => parse_flag!(report.include_build_script),

                // show-env options
                Long(flag @ ("sh" | "export-prefix")) => {
                    if flag == "export-prefix" {
                        renamed(flag, "--sh");
                    }
                    parse_flag!(sh);
                }
                Long(flag @ ("pwsh" | "with-pwsh-env-prefix")) => {
                    if flag == "with-pwsh-env-prefix" {
                        renamed(flag, "--pwsh");
                    }
                    parse_flag!(pwsh);
                }
                Long("cmd") => parse_flag!(cmd),
                Long("csh") => parse_flag!(csh),
                Long("fish") => parse_flag!(fish),
                Long("nu") => parse_flag!(nu),
                Long("xonsh") => parse_flag!(xonsh),

                // ambiguous between nextest-related and others will be handled later
                Long("archive-file") => parse_opt_passthrough!(archive_file),
                Long("nextest-archive-file") => parse_opt!(nextest_archive_file),

                Short('v') | Long("verbose") => {
                    verbose += 1;
                    after_subcommand = false;
                }
                Short('h') | Long("help") => {
                    print!("{}", Subcommand::help_text(subcommand));
                    return Ok(None);
                }
                Short('V') | Long("version") => {
                    if subcommand == Subcommand::None {
                        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                        return Ok(None);
                    }
                    unexpected("--version", subcommand)?;
                }

                // TODO: Currently, we are using a subdirectory of the target directory as
                //       the actual target directory. What effect should this option have
                //       on its behavior?
                Long("target-dir") => unexpected(&format_arg(&arg), subcommand)?,

                // Handle known options for can_passthrough=false subcommands
                Short('Z') => parse_opt_passthrough!(()),
                Short('F' | 'j') | Long("features" | "jobs")
                    if matches!(
                        subcommand,
                        Subcommand::None
                            | Subcommand::Test
                            | Subcommand::Run
                            | Subcommand::Nextest { .. }
                            | Subcommand::NextestArchive
                    ) =>
                {
                    parse_opt_passthrough!(());
                }
                Short('q') | Long("quiet") => passthrough!(),
                Long(
                    "all-features"
                    | "no-default-features"
                    | "--keep-going"
                    | "--ignore-rust-version",
                ) if matches!(
                    subcommand,
                    Subcommand::None
                        | Subcommand::Test
                        | Subcommand::Run
                        | Subcommand::Nextest { .. }
                        | Subcommand::NextestArchive
                ) =>
                {
                    passthrough!();
                }

                // passthrough
                Long(_) | Short(_) if Subcommand::can_passthrough(subcommand) => passthrough!(),
                Value(val)
                    if subcommand == Subcommand::None
                        || Subcommand::can_passthrough(subcommand) =>
                {
                    let val = val.into_string().unwrap();
                    if subcommand == Subcommand::None {
                        subcommand = val.parse::<Subcommand>()?;
                        after_subcommand = true;
                    } else {
                        if after_subcommand
                            && matches!(subcommand, Subcommand::Nextest { .. })
                            && matches!(
                                val.as_str(),
                                // from `cargo nextest --help`
                                "list" | "run" | "archive" | "show-config" | "self" | "help"
                            )
                        {
                            // The following warning is a hint, so it should not be promoted to an error.
                            let _guard = term::warn::ignore();
                            warn!(
                                "note that `{val}` is treated as test filter instead of subcommand \
                                 because `cargo llvm-cov nextest` internally calls \
                                 `cargo nextest run`; if you want to use `nextest archive`, \
                                 please use `cargo llvm-cov nextest-archive`"
                            );
                        }
                        cargo_args.push(val);
                        after_subcommand = false;
                    }
                }
                _ => unexpected(&format_arg(&arg), subcommand)?,
            }
        }

        term::set_coloring(&mut color);

        // ---------------------------------------------------------------------
        // Arguments validations

        // Handle options specific to certain operations.
        // report specific
        report.validate(subcommand)?;
        // clean specific
        clean.validate(subcommand)?;
        // show-env specific
        let show_env = ShowEnvOptions::new(subcommand, sh, pwsh, cmd, csh, fish, nu, xonsh)?;
        // test or show-env or report specific
        if doc || doctests {
            match subcommand {
                Subcommand::None | Subcommand::Test => {}
                Subcommand::ShowEnv | Subcommand::Report { .. } => {
                    // TODO: reject --doc
                    if !doctests {
                        specific_flag("--doc", subcommand, &["test", ""])?;
                    }
                }
                Subcommand::Nextest { .. } | Subcommand::NextestArchive => {
                    bail!(
                        "doctest is not supported for nextest; see <https://github.com/nextest-rs/nextest/issues/16> for more"
                    )
                }
                _ => {
                    if doc {
                        specific_flag("--doc", subcommand, &["test", ""])?;
                    } else {
                        specific_flag("--doctests", subcommand, &[
                            "test", "show-env", "report", "",
                        ])?;
                    }
                }
            }
        }
        // test or nextest specific
        match subcommand {
            Subcommand::None
            | Subcommand::Test
            | Subcommand::Nextest { .. }
            | Subcommand::NextestArchive => {}
            Subcommand::Run
            | Subcommand::Clean
            | Subcommand::Report { .. }
            | Subcommand::ShowEnv => {
                for (flag, passed) in [
                    ("--lib", lib),
                    ("--bins", bins),
                    ("--examples", examples),
                    ("--test", !test.is_empty()),
                    ("--tests", tests),
                    ("--bench", !bench.is_empty()),
                    ("--benches", benches),
                    ("--all-targets", all_targets),
                    ("--no-fail-fast", no_fail_fast),
                    ("--exclude", !exclude.is_empty()), // TODO: allow for report subcommand
                    ("--exclude-from-test", !exclude_from_test.is_empty()),
                ] {
                    if passed {
                        specific_flag(flag, subcommand, &[
                            "test",
                            "nextest",
                            "nextest-archive",
                            "",
                        ])?;
                    }
                }
            }
        }
        if no_run {
            match subcommand {
                Subcommand::None | Subcommand::Nextest { .. } | Subcommand::NextestArchive => {
                    // The following warnings should not be promoted to an error.
                    let _guard = term::warn::ignore();
                    warn!("--no-run is deprecated, use `cargo llvm-cov report` subcommand instead");
                }
                _ => {
                    specific_flag("--no-run", subcommand, &["nextest", "nextest-archive", ""])?;
                }
            }
        }
        // test or nextest or run specific
        match subcommand {
            Subcommand::None
            | Subcommand::Test
            | Subcommand::Run
            | Subcommand::Nextest { .. }
            | Subcommand::NextestArchive => {}
            Subcommand::Report { .. } | Subcommand::Clean | Subcommand::ShowEnv => {
                for (flag, passed) in [
                    ("--bin", !bin.is_empty()),
                    ("--example", !example.is_empty()),
                    // --exclude for report subcommand means "exclude from report"
                    ("--exclude-from-report", !exclude_from_report.is_empty()),
                    ("--ignore-run-fail", ignore_run_fail),
                ] {
                    if passed {
                        specific_flag(flag, subcommand, &[
                            "test",
                            "run",
                            "nextest",
                            "nextest-archive",
                            "",
                        ])?;
                    }
                }
            }
        }
        // test or nextest or run or show-env specific
        match subcommand {
            Subcommand::None
            | Subcommand::Test
            | Subcommand::Run
            | Subcommand::Nextest { .. }
            | Subcommand::NextestArchive
            | Subcommand::ShowEnv => {}
            Subcommand::Report { .. } | Subcommand::Clean => {
                for (flag, passed) in [
                    ("--no-cfg-coverage", no_cfg_coverage),
                    ("--no-cfg-coverage-nightly", no_cfg_coverage_nightly),
                    ("--no-rustc-wrapper", no_rustc_wrapper),
                ] {
                    if passed {
                        specific_flag(flag, subcommand, &[
                            "test",
                            "run",
                            "nextest",
                            "nextest-archive",
                            "show-env",
                            "",
                        ])?;
                    }
                }
            }
        }
        // test or nextest or clean specific
        match subcommand {
            Subcommand::None
            | Subcommand::Test
            | Subcommand::Nextest { .. }
            | Subcommand::NextestArchive
            | Subcommand::Clean => {}
            Subcommand::Run | Subcommand::Report { .. } | Subcommand::ShowEnv => {
                // TODO: allow report?
                if workspace {
                    specific_flag("--workspace", subcommand, &[
                        "test",
                        "nextest",
                        "nextest-archive",
                        "show-env",
                        "clean",
                        "",
                    ])?;
                }
            }
        }
        // nextest-related
        if subcommand.call_cargo_nextest() {
            if let Some(profile) = profile {
                // nextest profile will be propagated
                cargo_args.push("--profile".to_owned());
                cargo_args.push(profile);
            }
            if nextest_archive_file.is_some() {
                bail!(
                    "'--nextest-archive-file' is report-specific option; \
                    consider using '--archive-file' for nextest subcommands"
                );
            }
            nextest_archive_file = archive_file;
            if let Subcommand::Nextest { archive_file: f } = &mut subcommand {
                *f = nextest_archive_file.is_some();
            }
        } else {
            if cargo_profile.is_some() {
                bail!(
                    "'--cargo-profile' is nextest-specific option; \
                     consider using '--profile' instead for non-nextest subcommands"
                );
            }
            cargo_profile = profile;
            if let Subcommand::Report { nextest_archive_file: f } = &mut subcommand {
                if archive_file.is_some() {
                    bail!(
                        "'--archive-file' is nextest-specific option; \
                         consider using '--nextest-archive-file instead for report subcommand'"
                    );
                }
                *f = nextest_archive_file.is_some();
            } else {
                if archive_file.is_some() {
                    specific_flag("--archive-file", subcommand, &["nextest", "nextest-archive"])?;
                }
                if nextest_archive_file.is_some() {
                    specific_flag("--nextest-archive-file", subcommand, &["report"])?;
                }
            }
        }
        // TODO: check more

        // requires
        if !workspace {
            // TODO: This is the same behavior as cargo, but should we allow it to be used
            // in the root of a virtual workspace as well?
            if !exclude.is_empty() {
                requires("--exclude", &["--workspace"])?;
            }
            if !exclude_from_test.is_empty() {
                requires("--exclude-from-test", &["--workspace"])?;
            }
        }
        if coverage_target_only && target.is_none() {
            requires("--coverage-target-only", &["--target"])?;
        }

        // conflicts
        if report.no_report && no_run {
            conflicts("--no-report", "--no-run")?;
        }
        if report.no_report || no_run {
            let flag = if report.no_report { "--no-report" } else { "--no-run" };
            if clean.no_clean {
                // --no-report/--no-run implicitly enable --no-clean.
                conflicts(flag, "--no-clean")?;
            }
        }
        if ignore_run_fail && no_fail_fast {
            // --ignore-run-fail implicitly enable --no-fail-fast.
            conflicts("--ignore-run-fail", "--no-fail-fast")?;
        }
        if doc || doctests {
            let doc_flag = if doc { "--doc" } else { "--doctests" };
            for (flag, passed) in [
                ("--lib", lib),
                ("--bin", !bin.is_empty()),
                ("--bins", bins),
                ("--example", !example.is_empty()),
                ("--examples", examples),
                ("--test", !test.is_empty()),
                ("--tests", tests),
                ("--bench", !bench.is_empty()),
                ("--benches", benches),
                ("--all-targets", all_targets),
            ] {
                if passed {
                    conflicts(flag, doc_flag)?;
                }
            }
        }
        if workspace {
            if !package.is_empty() {
                // cargo allows the combination of --package and --workspace, but
                // we reject it because the situation where both flags are specified is odd.
                conflicts("--package", "--workspace")?;
            }
            if clean.profraw_only {
                conflicts_warn("--profraw-only", "--workspace");
            }
        }
        if branch && mcdc {
            conflicts("--branch", "--mcdc")?;
        }
        if subcommand.read_nextest_archive() {
            for (flag, passed) in [
                ("--target", target.is_some()),
                ("--release", release),
                ("--cargo-profile", cargo_profile.is_some()),
            ] {
                if passed {
                    info!(
                        "{flag} is no longer needed because detection from nextest archive is now supported"
                    );
                }
            }
        }

        // forbid_empty_values
        for (flag, is_empty) in [
            ("--ignore-filename-regex", report.ignore_filename_regex.as_deref() == Some("")),
            ("--output-path", report.output_path.as_deref() == Some(Utf8Path::new(""))),
            ("--output-dir", report.output_dir.as_deref() == Some(Utf8Path::new(""))),
        ] {
            if is_empty {
                bail!("empty string is not allowed in {flag}")
            }
        }

        for e in exclude {
            if exclude_from_test.contains(&e) {
                info!(
                    "--exclude-from-test {e} is needless because it is also specified by --exclude"
                );
            } else {
                // No need to push to exclude_from_test because already contained in cargo_args
            }
            if exclude_from_report.contains(&e) {
                info!(
                    "--exclude-from-report {e} is needless because it is also specified by --exclude"
                );
            } else {
                exclude_from_report.push(e);
            }
        }

        {
            // The following warnings should not be promoted to an error.
            let _guard = term::warn::ignore();
            if branch {
                warn!("--branch option is unstable");
            }
            if mcdc {
                warn!("--mcdc option is unstable");
            }
            if doc {
                warn!("--doc option is unstable");
            }
            if doctests {
                warn!("--doctests option is unstable");
            }
        }
        if coverage_target_only {
            info!(
                "when --coverage-target-only flag is used, coverage for proc-macro and build script will \
                 not be displayed"
            );
        } else if no_rustc_wrapper && target.is_some() {
            info!(
                "When both --no-rustc-wrapper flag and --target option are used, coverage for proc-macro and \
                 build script will not be displayed because cargo does not pass RUSTFLAGS to them"
            );
        }
        if no_rustc_wrapper && !dep_coverage.is_empty() {
            warn!("--dep-coverage may not work together with --no-rustc-wrapper");
        }

        // ---------------------------------------------------------------------
        // Preparation for subsequent processing

        // If `-vv` is passed, propagate `-v` to cargo.
        if verbose > 1 {
            cargo_args.push(format!("-{}", "v".repeat(verbose - 1)));
        }
        // --no-report and --no-run implies --no-clean
        clean.no_clean |= report.no_report | no_run;
        // --doc implies --doctests
        doctests |= doc;
        // --open implies --html
        report.html |= report.open;
        if no_run {
            // --no-run is deprecated alias for report
            subcommand = Subcommand::Report { nextest_archive_file: false };
        }
        if report.output_dir.is_some() && !report.show() {
            // If the format flag is not specified, this flag is no-op.
            // TODO: warn
            report.output_dir = None;
        }

        Ok(Some((
            Self {
                subcommand,
                build: BuildOptions {
                    ignore_run_fail,
                    has_target_selection_options: lib
                        | bins
                        | examples
                        | tests
                        | benches
                        | all_targets
                        | doc
                        | !bin.is_empty()
                        | !example.is_empty()
                        | !test.is_empty()
                        | !bench.is_empty(),
                    exclude_from_test,
                    branch,
                    mcdc,
                    include_ffi,
                    coverage_target_only,
                    no_cfg_coverage,
                    no_cfg_coverage_nightly,
                    no_rustc_wrapper,
                    cargo_args,
                    rest,
                },
                report,
                clean,
                show_env,
                doctests,
                workspace,
                release,
                cargo_profile,
                target,
                verbose: verbose.try_into().unwrap_or(u8::MAX),
                remap_path_prefix,
                dep_coverage,
                nextest_archive_file,
            },
            UnresolvedArgs { package, exclude_from_report, manifest_path, color },
        )))
    }
}

trait Store<T> {
    fn is_full(&self) -> bool {
        false
    }
    fn push(&mut self, val: &str) -> Result<()>;
}
impl Store<OsString> for () {
    fn push(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
}
impl<T: FromStr> Store<T> for Option<T>
where
    Error: From<T::Err>,
{
    fn is_full(&self) -> bool {
        self.is_some()
    }
    fn push(&mut self, val: &str) -> Result<()> {
        *self = Some(val.parse()?);
        Ok(())
    }
}
impl<T: FromStr> Store<T> for Vec<T>
where
    Error: From<T::Err>,
{
    fn push(&mut self, val: &str) -> Result<()> {
        self.push(val.parse()?);
        Ok(())
    }
}

fn format_arg(arg: &lexopt::Arg<'_>) -> String {
    match arg {
        Long(flag) => format!("--{flag}"),
        Short(flag) => format!("-{flag}"),
        Value(val) => val.parse().unwrap(),
    }
}

#[cold]
#[inline(never)]
fn multi_arg(flag: &lexopt::Arg<'_>) -> Result<()> {
    let flag = &format_arg(flag);
    bail!("the argument '{flag}' was provided more than once, but cannot be used multiple times");
}

// `flag` requires one of `requires`.
#[cold]
#[inline(never)]
fn requires(flag: &str, requires: &[&str]) -> Result<()> {
    let with = match requires.len() {
        0 => unreachable!(),
        1 => requires[0].to_owned(),
        2 => format!("either {} or {}", requires[0], requires[1]),
        _ => {
            let mut with = String::new();
            for f in requires.iter().take(requires.len() - 1) {
                with += f;
                with += ", ";
            }
            with += "or ";
            with += requires.last().unwrap();
            with
        }
    };
    bail!("{flag} can only be used together with {with}");
}

#[cold]
fn conflicts_msg(a: &str, b: &str) -> String {
    format!("{a} may not be used together with {b}")
}
#[cold]
#[inline(never)]
fn conflicts(a: &str, b: &str) -> Result<()> {
    Err(Error::msg(conflicts_msg(a, b)))
}
// TODO(semver): replace this with conflicts on future breaking release
#[cold]
#[inline(never)]
fn conflicts_warn(a: &str, b: &str) {
    warn!("{}", conflicts_msg(a, b));
}

#[cold]
#[inline(never)]
fn unexpected(arg: &str, subcommand: Subcommand) -> Result<()> {
    if arg.starts_with('-') && !arg.starts_with("---") && arg != "--" {
        if subcommand == Subcommand::None {
            bail!("invalid option '{arg}'");
        }
        bail!("invalid option '{arg}' for subcommand '{}'", subcommand.as_str());
    }
    Err(lexopt::Error::UnexpectedArgument(arg.into()).into())
}

#[cold]
fn specific_flag_msg(flag: &str, subcommand: Subcommand, specific_to: &[&str]) -> String {
    assert!(!specific_to.is_empty());
    assert!(flag.starts_with('-') && !flag.starts_with("---") && flag != "--");
    let mut list = String::new();
    if specific_to.len() != 1 {
        list.push('[');
    }
    for subcmd in specific_to {
        if subcmd.is_empty() {
            list.push_str("no subcommand");
        } else {
            list.push_str(subcmd);
        }
        list.push(',');
    }
    list.pop(); // drop trailing comma
    if specific_to.len() != 1 {
        list.push(']');
    }
    if subcommand == Subcommand::None {
        format!("{flag} is specific to {list}")
    } else {
        format!(
            "{flag} is specific to {list} and not supported for subcommand '{}'",
            subcommand.as_str()
        )
    }
}
#[cold]
#[inline(never)]
fn specific_flag(flag: &str, subcommand: Subcommand, specific_to: &[&str]) -> Result<()> {
    Err(Error::msg(specific_flag_msg(flag, subcommand, specific_to)))
}
// TODO(semver): replace this with conflicts on future breaking release
#[cold]
#[inline(never)]
fn specific_flag_warn(flag: &str, subcommand: Subcommand, specific_to: &[&str]) {
    warn!("{}", specific_flag_msg(flag, subcommand, specific_to));
}

#[cold]
#[inline(never)]
fn renamed(from: &str, to: &str) {
    warn!(
        "{from} has been renamed to {to}; \
         old name is still available as an alias, but may removed in \
         future breaking release"
    );
}
