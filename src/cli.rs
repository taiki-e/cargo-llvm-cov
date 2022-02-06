use std::mem;

use camino::Utf8PathBuf;
use clap::{AppSettings, ArgSettings, Parser};

use crate::{process::ProcessBuilder, term::Coloring};

const ABOUT: &str =
    "Cargo subcommand to easily use LLVM source-based code coverage (-C instrument-coverage).

Use -h for short descriptions and --help for more details.";

const MAX_TERM_WIDTH: usize = 100;

#[derive(Debug, Parser)]
#[clap(
    bin_name = "cargo",
    version,
    max_term_width(MAX_TERM_WIDTH),
    setting(AppSettings::DeriveDisplayOrder)
)]
pub(crate) enum Opts {
    #[clap(about(ABOUT), version)]
    LlvmCov(Args),
}

#[derive(Debug, Parser)]
#[clap(
    bin_name = "cargo llvm-cov",
    about(ABOUT),
    version,
    max_term_width(MAX_TERM_WIDTH),
    setting(AppSettings::DeriveDisplayOrder)
)]
pub(crate) struct Args {
    #[clap(subcommand)]
    pub(crate) subcommand: Option<Subcommand>,

    #[clap(flatten)]
    cov: LlvmCovOptions,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    ///
    /// This flag is unstable.
    /// See <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for more.
    #[clap(long)]
    pub(crate) doctests: bool,

    // =========================================================================
    // `cargo test` options
    // https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html
    /// Generate coverage report without running tests
    #[clap(long, conflicts_with = "no-report")]
    pub(crate) no_run: bool,
    /// Run all tests regardless of failure
    #[clap(long)]
    pub(crate) no_fail_fast: bool,
    /// Display one character per test instead of one line
    #[clap(short, long, conflicts_with = "verbose")]
    pub(crate) quiet: bool,
    /// Test only this package's library unit tests
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) lib: bool,
    /// Test only the specified binary
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "NAME",
        conflicts_with = "doc",
        conflicts_with = "doctests"
    )]
    pub(crate) bin: Vec<String>,
    /// Test all binaries
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) bins: bool,
    /// Test only the specified example
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "NAME",
        conflicts_with = "doc",
        conflicts_with = "doctests"
    )]
    pub(crate) example: Vec<String>,
    /// Test all examples
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) examples: bool,
    /// Test only the specified test target
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "NAME",
        conflicts_with = "doc",
        conflicts_with = "doctests"
    )]
    pub(crate) test: Vec<String>,
    /// Test all tests
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) tests: bool,
    /// Test only the specified bench target
    #[clap(
        long,
        multiple_occurrences = true,
        value_name = "NAME",
        conflicts_with = "doc",
        conflicts_with = "doctests"
    )]
    pub(crate) bench: Vec<String>,
    /// Test all benches
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) benches: bool,
    /// Test all targets
    #[clap(long, conflicts_with = "doc", conflicts_with = "doctests")]
    pub(crate) all_targets: bool,
    /// Test only this library's documentation (unstable)
    ///
    /// This flag is unstable because it automatically enables --doctests flag.
    /// See <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for more.
    #[clap(long)]
    pub(crate) doc: bool,
    /// Package to run tests for
    // cargo allows the combination of --package and --workspace, but we reject
    // it because the situation where both flags are specified is odd.
    #[clap(
        short,
        long,
        multiple_occurrences = true,
        multiple_values = true,
        value_name = "SPEC",
        conflicts_with = "workspace"
    )]
    pub(crate) package: Vec<String>,
    /// Test all packages in the workspace
    #[clap(long, visible_alias = "all")]
    pub(crate) workspace: bool,
    /// Exclude packages from the test
    #[clap(
        long,
        multiple_occurrences = true,
        multiple_values = true,
        value_name = "SPEC",
        requires = "workspace"
    )]
    pub(crate) exclude: Vec<String>,

    #[clap(flatten)]
    build: BuildOptions,

    #[clap(flatten)]
    manifest: ManifestOptions,

    /// Unstable (nightly-only) flags to Cargo
    #[clap(short = 'Z', multiple_occurrences = true, value_name = "FLAG")]
    pub(crate) unstable_flags: Vec<String>,

    /// Arguments for the test binary
    #[clap(last = true)]
    pub(crate) args: Vec<String>,
}

impl Args {
    pub(crate) fn cov(&mut self) -> LlvmCovOptions {
        mem::take(&mut self.cov)
    }

    pub(crate) fn build(&mut self) -> BuildOptions {
        mem::take(&mut self.build)
    }

    pub(crate) fn manifest(&mut self) -> ManifestOptions {
        mem::take(&mut self.manifest)
    }
}

#[derive(Debug, Parser)]
pub(crate) enum Subcommand {
    /// Run a binary or example and generate coverage report.
    #[clap(
        bin_name = "cargo llvm-cov run",
        max_term_width(MAX_TERM_WIDTH),
        setting(AppSettings::DeriveDisplayOrder)
    )]
    Run(Box<RunOptions>),

    /// Output the environment set by cargo-llvm-cov to build Rust projects.
    #[clap(
        bin_name = "cargo llvm-cov show-env",
        max_term_width(MAX_TERM_WIDTH),
        setting(AppSettings::DeriveDisplayOrder)
    )]
    ShowEnv(ShowEnvOptions),

    /// Remove artifacts that cargo-llvm-cov has generated in the past
    #[clap(
        bin_name = "cargo llvm-cov clean",
        max_term_width(MAX_TERM_WIDTH),
        setting(AppSettings::DeriveDisplayOrder)
    )]
    Clean(CleanOptions),

    // internal (unstable)
    #[clap(
        bin_name = "cargo llvm-cov demangle",
        max_term_width(MAX_TERM_WIDTH),
        setting(AppSettings::DeriveDisplayOrder),
        setting(AppSettings::Hidden)
    )]
    Demangle,
}

#[derive(Debug, Default, Parser)]
pub(crate) struct LlvmCovOptions {
    /// Export coverage data in "json" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[clap(long)]
    pub(crate) json: bool,
    /// Export coverage data in "lcov" format
    ///
    /// If --output-path is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov export -format=lcov`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[clap(long, conflicts_with = "json")]
    pub(crate) lcov: bool,

    /// Generate coverage report in “text” format
    ///
    /// If --output-path or --output-dir is not specified, the report will be printed to stdout.
    ///
    /// This internally calls `llvm-cov show -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[clap(long, conflicts_with = "json", conflicts_with = "lcov")]
    pub(crate) text: bool,
    /// Generate coverage report in "html" format
    ///
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

    /// Export only summary information for each file in the coverage data
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
    /// Specify a directory to write coverage report into (default to `target/llvm-cov`).
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

    /// Fail if `any` or `all` profiles cannot be merged (default to `any`)
    #[clap(long, value_name = "any|all", possible_values(&["any", "all"]), hide_possible_values = true)]
    pub(crate) failure_mode: Option<String>,
    /// Skip source code files with file paths that match the given regular expression.
    #[clap(long, value_name = "PATTERN", setting(ArgSettings::ForbidEmptyValues))]
    pub(crate) ignore_filename_regex: Option<String>,
    // For debugging (unstable)
    #[clap(long, hide = true)]
    pub(crate) disable_default_ignore_filename_regex: bool,
    // For debugging (unstable)
    /// Hide instantiations from report
    #[clap(long, hide = true)]
    pub(crate) hide_instantiations: bool,
    // For debugging (unstable)
    /// Unset cfg(coverage)
    #[clap(long, hide = true)]
    pub(crate) no_cfg_coverage: bool,
    /// Run tests, but don't generate coverage report
    #[clap(long)]
    pub(crate) no_report: bool,
}

impl LlvmCovOptions {
    pub(crate) fn show(&self) -> bool {
        self.text || self.html
    }
}

#[derive(Debug, Default, Parser)]
pub(crate) struct BuildOptions {
    /// Number of parallel jobs, defaults to # of CPUs
    // Max value is u32::MAX: https://github.com/rust-lang/cargo/blob/0.55.0/src/cargo/util/command_prelude.rs#L332
    #[clap(short, long, value_name = "N")]
    pub(crate) jobs: Option<u32>,
    /// Build artifacts in release mode, with optimizations
    #[clap(long)]
    pub(crate) release: bool,
    /// Build artifacts with the specified profile
    // TODO: this option is not fully handled yet
    // https://github.com/rust-lang/cargo/issues/6988
    // https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#custom-named-profiles
    #[clap(long, value_name = "PROFILE-NAME")]
    pub(crate) profile: Option<String>,
    /// Space or comma separated list of features to activate
    #[clap(long, multiple_occurrences = true, value_name = "FEATURES")]
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
    /// Use verbose output
    ///
    /// Use -vv (-vvv) to propagate verbosity to cargo.
    #[clap(short, long, parse(from_occurrences))]
    pub(crate) verbose: u8,
    /// Coloring
    // This flag will be propagated to both cargo and llvm-cov.
    #[clap(long, arg_enum, value_name = "WHEN")]
    pub(crate) color: Option<Coloring>,
}

impl BuildOptions {
    pub(crate) fn cargo_args(&self, cmd: &mut ProcessBuilder) {
        if let Some(jobs) = self.jobs {
            cmd.arg("--jobs");
            cmd.arg(jobs.to_string());
        }
        if self.release {
            cmd.arg("--release");
        }
        if let Some(profile) = &self.profile {
            cmd.arg("--profile");
            cmd.arg(profile);
        }
        for features in &self.features {
            cmd.arg("--features");
            cmd.arg(features);
        }
        if self.all_features {
            cmd.arg("--all-features");
        }
        if self.no_default_features {
            cmd.arg("--no-default-features");
        }
        if let Some(target) = &self.target {
            cmd.arg("--target");
            cmd.arg(target);
        }

        if let Some(color) = self.color {
            cmd.arg("--color");
            cmd.arg(color.cargo_color());
        }

        // If `-vv` is passed, propagate `-v` to cargo.
        if self.verbose > 1 {
            cmd.arg(format!("-{}", "v".repeat(self.verbose as usize - 1)));
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct RunOptions {
    #[clap(flatten)]
    cov: LlvmCovOptions,

    /// No output printed to stdout
    #[clap(short, long, conflicts_with = "verbose")]
    pub(crate) quiet: bool,
    /// Name of the bin target to run
    #[clap(long, multiple_occurrences = true, value_name = "NAME")]
    pub(crate) bin: Vec<String>,
    /// Name of the example target to run
    #[clap(long, multiple_occurrences = true, value_name = "NAME")]
    pub(crate) example: Vec<String>,
    /// Package with the target to run
    #[clap(short, long, value_name = "SPEC")]
    pub(crate) package: Option<String>,

    #[clap(flatten)]
    build: BuildOptions,

    #[clap(flatten)]
    manifest: ManifestOptions,

    /// Unstable (nightly-only) flags to Cargo
    #[clap(short = 'Z', multiple_occurrences = true, value_name = "FLAG")]
    pub(crate) unstable_flags: Vec<String>,

    /// Arguments for the test binary
    #[clap(last = true)]
    pub(crate) args: Vec<String>,
}

impl RunOptions {
    pub(crate) fn cov(&mut self) -> LlvmCovOptions {
        mem::take(&mut self.cov)
    }

    pub(crate) fn build(&mut self) -> BuildOptions {
        mem::take(&mut self.build)
    }

    pub(crate) fn manifest(&mut self) -> ManifestOptions {
        mem::take(&mut self.manifest)
    }
}

#[derive(Debug, Parser)]
pub(crate) struct ShowEnvOptions {
    /// Prepend "export " to each line, so that the output is suitable to be sourced by bash.
    #[clap(long)]
    pub(crate) export_prefix: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct CleanOptions {
    /// Remove artifacts that may affect the coverage results of packages in the workspace.
    #[clap(long)]
    pub(crate) workspace: bool,
    // TODO: Currently, we are using a subdirectory of the target directory as
    //       the actual target directory. What effect should this option have
    //       on its behavior?
    // /// Directory for all generated artifacts
    // #[clap(long, value_name = "DIRECTORY")]
    // pub(crate) target_dir: Option<Utf8PathBuf>,
    /// Use verbose output
    #[clap(short, long, parse(from_occurrences))]
    pub(crate) verbose: u8,
    /// Coloring
    #[clap(long, arg_enum, value_name = "WHEN")]
    pub(crate) color: Option<Coloring>,
    #[clap(flatten)]
    pub(crate) manifest: ManifestOptions,
}

// https://doc.rust-lang.org/nightly/cargo/commands/cargo-test.html#manifest-options
#[derive(Debug, Default, Parser)]
pub(crate) struct ManifestOptions {
    /// Path to Cargo.toml
    #[clap(long, value_name = "PATH")]
    pub(crate) manifest_path: Option<Utf8PathBuf>,
    /// Require Cargo.lock and cache are up to date
    #[clap(long)]
    pub(crate) frozen: bool,
    /// Require Cargo.lock is up to date
    #[clap(long)]
    pub(crate) locked: bool,
    /// Run without accessing the network
    #[clap(long)]
    pub(crate) offline: bool,
}

impl ManifestOptions {
    pub(crate) fn cargo_args(&self, cmd: &mut ProcessBuilder) {
        // Skip --manifest-path because it is set based on Workspace::current_manifest.
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

#[cfg(test)]
mod tests {
    use std::{
        env,
        io::Write,
        panic,
        path::Path,
        process::{Command, Stdio},
    };

    use anyhow::Result;
    use clap::{IntoApp, Parser};
    use fs_err as fs;

    use super::{Args, Opts, MAX_TERM_WIDTH};

    #[test]
    fn assert_app() {
        Args::into_app().debug_assert();
    }

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
        assert_eq!(args.build.features, ["a", "b"]);

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
        let forbidden = &[
            "--output-path",
            "--output-dir",
            "--ignore-filename-regex",
            // "--target-dir",
        ];
        let allowed = &[
            "--bin",
            "--example",
            "--test",
            "--bench",
            "--package",
            "--exclude",
            "--profile",
            "--features",
            "--target",
            // "--target-dir",
            "--manifest-path",
            "-Z",
            "--",
        ];

        for &flag in forbidden {
            Opts::try_parse_from(&["cargo", "llvm-cov", flag, ""]).unwrap_err();
        }
        for &flag in allowed {
            if flag == "--exclude" {
                Opts::try_parse_from(&["cargo", "llvm-cov", flag, "", "--workspace"]).unwrap();
            } else {
                Opts::try_parse_from(&["cargo", "llvm-cov", flag, ""]).unwrap();
            }
        }
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
        let manifest_dir =
            manifest_dir.strip_prefix(env::current_dir().unwrap()).unwrap_or(manifest_dir);
        let expected_path = &manifest_dir.join(expected_path);
        if !expected_path.is_file() {
            fs::write(expected_path, "").unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        if expected != actual {
            if env::var_os("CI").is_some() {
                let mut child = Command::new("git")
                    .args(["--no-pager", "diff", "--no-index", "--"])
                    .arg(expected_path)
                    .arg("-")
                    .stdin(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.as_mut().unwrap().write_all(actual.as_bytes()).unwrap();
                assert!(!child.wait().unwrap().success());
                // patch -p1 <<'EOF' ... EOF
                panic!("assertion failed; please run test locally and commit resulting changes, or apply above diff as patch");
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
            assert_diff(path, out);
        } else if start {
            panic!("missing `<!-- readme-long-help:end -->` comment in README.md");
        } else {
            panic!("missing `<!-- readme-long-help:start -->` comment in README.md");
        }
        Ok(())
    }
}
