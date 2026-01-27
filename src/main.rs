// SPDX-License-Identifier: Apache-2.0 OR MIT

#![forbid(unsafe_code)]

// Refs:
// - https://doc.rust-lang.org/nightly/rustc/instrument-coverage.html

use std::{
    ffi::OsStr,
    fmt::Write as _,
    io::{self, BufWriter, Write as _},
    process::ExitCode,
};

use anyhow::{Context as _, Result, bail};
use cargo_config2::Flags;

use crate::{
    cli::{Args, ShowEnvOptions, Subcommand},
    context::Context,
    process::ProcessBuilder,
};

#[macro_use]
mod term;

#[macro_use]
mod process;

mod cargo;
mod clean;
mod cli;
mod context;
mod demangler;
mod env;
mod fs;
mod metadata;
mod regex_vec;
mod report;
mod wrapper;

fn main() -> ExitCode {
    term::init_coloring();
    let res = if demangler::is_enabled() {
        demangler::try_main()
    } else if wrapper::is_enabled() {
        wrapper::try_main()
    } else {
        try_main()
    };
    if let Err(e) = res {
        error!("{e:#}");
    }
    if term::error() || term::warn() && env::var_os("CARGO_LLVM_COV_DENY_WARNINGS").is_some() {
        process::last_failure_exit_code().unwrap_or(ExitCode::FAILURE)
    } else {
        ExitCode::SUCCESS
    }
}

fn try_main() -> Result<()> {
    let Some(args) = Args::parse()? else { return Ok(()) };
    term::verbose::set(args.0.verbose != 0);

    match args.0.subcommand {
        Subcommand::Clean => clean::run(args)?,
        Subcommand::ShowEnv => {
            let cx = &Context::new(args)?;
            let writer = &mut ShowEnvWriter {
                writer: BufWriter::new(io::stdout().lock()), // Buffered because it is written with newline many times.
                options: cx.args.show_env.clone(),
            };
            set_env(cx, writer, IsNextest(true))?; // Include env vars for nextest.
            writer.set("CARGO_LLVM_COV_TARGET_DIR", cx.ws.metadata.target_directory.as_str())?;
            writer.set("CARGO_LLVM_COV_BUILD_DIR", cx.ws.metadata.build_directory().as_str())?;
            writer.writer.flush()?;
        }
        Subcommand::Report { .. } => {
            let cx = &Context::new(args)?;
            report::generate(cx)?;
        }
        Subcommand::Run => {
            let cx = &Context::new(args)?;
            clean::clean_partial(cx)?;
            create_dirs_for_build(cx)?;
            run_run(cx)?;
            report::generate(cx)?;
        }
        Subcommand::Nextest { .. } => {
            let cx = &Context::new(args)?;
            clean::clean_partial(cx)?;
            create_dirs_for_build(cx)?;
            run_nextest(cx)?;
            report::generate(cx)?;
        }
        Subcommand::NextestArchive => {
            let cx = &Context::new(args)?;
            clean::clean_partial(cx)?;
            create_dirs_for_build(cx)?;
            archive_nextest(cx)?;
        }
        Subcommand::None | Subcommand::Test => {
            let cx = &Context::new(args)?;
            clean::clean_partial(cx)?;
            create_dirs_for_build(cx)?;
            run_test(cx)?;
            report::generate(cx)?;
        }
    }
    Ok(())
}

trait EnvTarget {
    fn set(&mut self, key: &str, value: &str) -> Result<()> {
        self.set_os(key, OsStr::new(value))
    }
    fn set_os(&mut self, key: &str, value: &OsStr) -> Result<()>;
    fn unset(&mut self, key: &str) -> Result<()>;
}

impl EnvTarget for ProcessBuilder {
    fn set_os(&mut self, key: &str, value: &OsStr) -> Result<()> {
        self.env(key, value);
        Ok(())
    }
    fn unset(&mut self, key: &str) -> Result<()> {
        self.env_remove(key);
        Ok(())
    }
}

struct ShowEnvWriter<W: io::Write> {
    writer: W,
    options: ShowEnvOptions,
}

impl<W: io::Write> EnvTarget for ShowEnvWriter<W> {
    fn set(&mut self, key: &str, value: &str) -> Result<()> {
        writeln!(self.writer, "{}", self.options.show_env_format.export_string(key, value))
            .context("failed to write env to stdout")
    }
    fn set_os(&mut self, key: &str, value: &OsStr) -> Result<()> {
        self.set(key, os_str_to_str(value)?)
    }
    fn unset(&mut self, key: &str) -> Result<()> {
        if env::var_os(key).is_some() {
            warn!("cannot unset environment variable `{key}`");
        }
        Ok(())
    }
}

struct IsNextest(bool);

fn set_env(cx: &Context, env: &mut dyn EnvTarget, IsNextest(is_nextest): IsNextest) -> Result<()> {
    fn push_common_flags(cx: &Context, flags: &mut Flags) {
        if cx.ws.stable_coverage {
            flags.push("-C");
            // TODO: if user already set -C instrument-coverage=..., respect it
            // https://doc.rust-lang.org/rustc/instrument-coverage.html#-c-instrument-coverageoptions
            flags.push("instrument-coverage");
        } else {
            flags.push("-Z");
            flags.push("instrument-coverage");
            if cx.ws.target_is_windows {
                // `-C codegen-units=1` is needed to work around link error on windows
                // https://github.com/rust-lang/rust/issues/85461
                // https://github.com/microsoft/windows-rs/issues/1006#issuecomment-887789950
                // This has been fixed in https://github.com/rust-lang/rust/pull/91470,
                // but old nightly compilers still need this.
                flags.push("-C");
                flags.push("codegen-units=1");
            }
        }
        if cx.args.mcdc {
            // Tracking issue: https://github.com/rust-lang/rust/issues/124144
            // TODO: Unstable MC/DC support has been removed in https://github.com/rust-lang/rust/pull/144999
            flags.push("-Z");
            flags.push("coverage-options=mcdc");
        } else if cx.args.branch {
            // Tracking issue: https://github.com/rust-lang/rust/issues/79649
            flags.push("-Z");
            flags.push("coverage-options=branch");
        }
        // Workaround for https://github.com/rust-lang/rust/issues/91092.
        // Unnecessary since https://github.com/rust-lang/rust/pull/111469.
        let needs_atomic_counter_workaround = if cx.ws.rustc_version.nightly {
            cx.ws.rustc_version.major_minor() <= (1, 71)
        } else {
            cx.ws.rustc_version.major_minor() < (1, 71)
        };
        if needs_atomic_counter_workaround {
            flags.push("-C");
            flags.push("llvm-args=--instrprof-atomic-counter-update-all");
        }
        if !cx.args.no_cfg_coverage {
            flags.push("--cfg=coverage");
        }
        if cx.ws.rustc_version.nightly && !cx.args.no_cfg_coverage_nightly {
            flags.push("--cfg=coverage_nightly");
        }
        if cx.ws.target_for_config.triple().ends_with("-windows-gnullvm") {
            // https://github.com/taiki-e/cargo-llvm-cov/issues/254#issuecomment-3700090953
            flags.push("-C");
            flags.push("link-arg=-Wl,--no-gc-sections");
        }
    }

    // Set LLVM_PROFILE_FILE.
    {
        let llvm_profile_file_name =
            if let Some(llvm_profile_file_name) = env::var("LLVM_PROFILE_FILE_NAME")? {
                if !llvm_profile_file_name.ends_with(".profraw") {
                    bail!("extension of LLVM_PROFILE_FILE_NAME must be 'profraw'");
                }
                llvm_profile_file_name
            } else {
                // TODO: remove %p (for nextest?) by default? https://github.com/taiki-e/cargo-llvm-cov/issues/335#issuecomment-1890349373
                let mut llvm_profile_file_name = format!("{}-%p", cx.ws.name);
                if is_nextest {
                    // https://github.com/taiki-e/cargo-llvm-cov/issues/258
                    // https://clang.llvm.org/docs/SourceBasedCodeCoverage.html#running-the-instrumented-program
                    // Select the number of threads that is the same as the one nextest uses by default here.
                    // https://github.com/nextest-rs/nextest/blob/c54694dfe7be016993983b5dedbcf2b50d4b1a6e/nextest-runner/src/config/test_threads.rs
                    // https://github.com/nextest-rs/nextest/blob/c54694dfe7be016993983b5dedbcf2b50d4b1a6e/nextest-runner/src/config/config_impl.rs#L30
                    // TODO: should we respect custom test-threads?
                    // - If the number of threads specified by the user is negative or
                    //   less or equal to available cores, it should not really be a problem
                    //   because it does not exceed the number of available cores.
                    // - Even if the number of threads specified by the user is greater than
                    //   available cores, it is expected that the number of threads that can
                    //   write simultaneously will not exceed the number of available cores.
                    let _ = write!(
                        llvm_profile_file_name,
                        "-%{}m",
                        // TODO: clamp to 1..=9?
                        // https://doc.rust-lang.org/rustc/instrument-coverage.html#running-the-instrumented-binary-to-generate-raw-coverage-profiling-data
                        // > N must be between 1 and 9
                        std::thread::available_parallelism().map_or(1, usize::from)
                    );
                } else {
                    llvm_profile_file_name.push_str("-%m");
                }
                llvm_profile_file_name.push_str(".profraw");
                llvm_profile_file_name
            };
        let llvm_profile_file = cx.ws.target_dir.join(llvm_profile_file_name);
        env.set("LLVM_PROFILE_FILE", llvm_profile_file.as_str())?;
    }

    // Set rustflags and related env vars.
    {
        let mut rustflags = Flags::default();
        push_common_flags(cx, &mut rustflags);
        if cx.args.remap_path_prefix {
            rustflags.push("--remap-path-prefix");
            rustflags.push(format!("{}/=", cx.ws.metadata.workspace_root));
        }
        wrapper::set_env(cx, env, &rustflags)?;
        if !wrapper::use_wrapper(cx) {
            if cx.args.target.is_none() {
                // cfg needed for trybuild support.
                // https://github.com/dtolnay/trybuild/pull/121
                // https://github.com/dtolnay/trybuild/issues/122
                // https://github.com/dtolnay/trybuild/pull/123
                rustflags.push("--cfg=trybuild_no_target");
            }
            let mut additional_flags = rustflags.flags;
            let mut rustflags =
                cx.ws.config.rustflags(&cx.ws.target_for_config)?.unwrap_or_default();
            rustflags.flags.append(&mut additional_flags);
            match (cx.args.coverage_target_only, &cx.args.target) {
                (true, Some(coverage_target)) => {
                    env.set(
                        &format!("CARGO_TARGET_{}_RUSTFLAGS", target_u_upper(coverage_target)),
                        &rustflags.encode_space_separated()?,
                    )?;
                    env.unset("RUSTFLAGS")?;
                    env.unset("CARGO_ENCODED_RUSTFLAGS")?;
                }
                _ => {
                    // First, try with RUSTFLAGS because `nextest` subcommand sometimes doesn't work well with encoded flags.
                    if let Ok(v) = rustflags.encode_space_separated() {
                        env.set("RUSTFLAGS", &v)?;
                        env.unset("CARGO_ENCODED_RUSTFLAGS")?;
                    } else {
                        env.set("CARGO_ENCODED_RUSTFLAGS", &rustflags.encode()?)?;
                    }
                }
            }
        }
    }

    // Set rustdocflags.
    // Note that rustdoc ignores rustc-wrapper: https://github.com/rust-lang/rust/issues/56232
    if cx.args.doctests {
        let mut rustdocflags =
            cx.ws.config.rustdocflags(&cx.ws.target_for_config)?.unwrap_or_default();
        {
            push_common_flags(cx, &mut rustdocflags);
            // flags needed for doctest coverage.
            // https://doc.rust-lang.org/nightly/rustc/instrument-coverage.html#including-doc-tests
            rustdocflags.push("-Z");
            rustdocflags.push("unstable-options");
            rustdocflags.push("--persist-doctests");
            rustdocflags.push(cx.ws.doctests_dir.as_str());
        }
        // First, try with RUSTDOCFLAGS because `nextest` subcommand sometimes doesn't work well with encoded flags.
        if let Ok(v) = rustdocflags.encode_space_separated() {
            env.set("RUSTDOCFLAGS", &v)?;
            env.unset("CARGO_ENCODED_RUSTDOCFLAGS")?;
        } else {
            env.set("CARGO_ENCODED_RUSTDOCFLAGS", &rustdocflags.encode()?)?;
        }
    }

    // Set env vars for FFI coverage.
    if cx.args.include_ffi {
        // https://github.com/rust-lang/cc-rs/blob/1.0.73/src/lib.rs#L2347-L2365
        // Environment variables that use hyphens are not available in many environments, so we ignore them for now.
        let target_u = target_u_lower(cx.ws.target_for_config.triple());
        let cflags_key = &format!("CFLAGS_{target_u}");
        // Use std::env instead of crate::env to match cc-rs's behavior.
        // https://github.com/rust-lang/cc-rs/blob/1.0.73/src/lib.rs#L2740
        let mut cflags = match std::env::var(cflags_key) {
            Ok(cflags) => cflags,
            Err(_) => match std::env::var("TARGET_CFLAGS") {
                Ok(cflags) => cflags,
                Err(_) => std::env::var("CFLAGS").unwrap_or_default(),
            },
        };
        let cxxflags_key = &format!("CXXFLAGS_{target_u}");
        let mut cxxflags = match std::env::var(cxxflags_key) {
            Ok(cxxflags) => cxxflags,
            Err(_) => match std::env::var("TARGET_CXXFLAGS") {
                Ok(cxxflags) => cxxflags,
                Err(_) => std::env::var("CXXFLAGS").unwrap_or_default(),
            },
        };
        let clang_flags = " -fprofile-instr-generate -fcoverage-mapping -fprofile-update=atomic";
        cflags.push_str(clang_flags);
        cxxflags.push_str(clang_flags);
        env.set(cflags_key, &cflags)?;
        env.set(cxxflags_key, &cxxflags)?;
    }

    // Set other env vars.
    env.set("CARGO_LLVM_COV", "1")?;
    if cx.args.subcommand == Subcommand::ShowEnv {
        env.set("CARGO_LLVM_COV_SHOW_ENV", "1")?;
    }
    Ok(())
}

fn has_z_flag(args: &[String], name: &str) -> bool {
    let mut iter = args.iter().map(String::as_str);
    while let Some(mut arg) = iter.next() {
        if arg == "-Z" {
            arg = iter.next().unwrap();
        } else if let Some(a) = arg.strip_prefix("-Z") {
            arg = a;
        } else {
            continue;
        }
        if let Some(rest) = arg.strip_prefix(name) {
            if rest.is_empty() || rest.starts_with('=') {
                return true;
            }
        }
    }
    false
}

fn create_dirs_for_build(cx: &Context) -> Result<()> {
    fs::create_dir_all(&cx.ws.target_dir)?;
    if cx.args.doctests {
        fs::create_dir_all(&cx.ws.doctests_dir)?;
    }
    Ok(())
}

fn run_test(cx: &Context) -> Result<()> {
    let mut cargo = cx.cargo();

    set_env(cx, &mut cargo, IsNextest(false))?;

    cargo.arg("test");
    if cx.ws.need_doctest_in_workspace && !has_z_flag(&cx.args.cargo_args, "doctest-in-workspace") {
        // https://github.com/rust-lang/cargo/issues/9427
        cargo.arg("-Z");
        cargo.arg("doctest-in-workspace");
    }

    if cx.args.ignore_run_fail {
        {
            let mut cargo = cargo.clone();
            cargo.arg("--no-run");
            cargo::test_or_run_args(cx, &mut cargo);
            if term::verbose() {
                status!("Running", "{cargo}");
                cargo.stdout_to_stderr().run()?;
            } else {
                // Capture output to prevent duplicate warnings from appearing in two runs.
                cargo.run_with_output()?;
            }
        }

        cargo.arg("--no-fail-fast");
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        if let Err(e) = cargo.run() {
            warn!("{e:#}");
        }
    } else {
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        cargo.run()?;
    }

    Ok(())
}

fn archive_nextest(cx: &Context) -> Result<()> {
    let mut cargo = cx.cargo();

    set_env(cx, &mut cargo, IsNextest(true))?;

    cargo.arg("nextest").arg("archive");

    cargo::test_or_run_args(cx, &mut cargo);
    if term::verbose() {
        status!("Running", "{cargo}");
    }
    stdout_to_stderr(cx, &mut cargo);
    cargo.run()?;

    Ok(())
}

fn run_nextest(cx: &Context) -> Result<()> {
    let mut cargo = cx.cargo();

    set_env(cx, &mut cargo, IsNextest(true))?;

    cargo.arg("nextest").arg("run");

    if cx.args.ignore_run_fail {
        {
            let mut cargo = cargo.clone();
            cargo.arg("--no-run");
            cargo::test_or_run_args(cx, &mut cargo);
            if term::verbose() {
                status!("Running", "{cargo}");
                cargo.stdout_to_stderr().run()?;
            } else {
                // Capture output to prevent duplicate warnings from appearing in two runs.
                cargo.run_with_output()?;
            }
        }

        cargo.arg("--no-fail-fast");
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        if let Err(e) = cargo.run() {
            warn!("{e:#}");
        }
    } else {
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        cargo.run()?;
    }
    Ok(())
}

fn run_run(cx: &Context) -> Result<()> {
    let mut cargo = cx.cargo();

    set_env(cx, &mut cargo, IsNextest(false))?;

    if cx.args.ignore_run_fail {
        {
            let mut cargo = cargo.clone();
            cargo.arg("build");
            cargo::test_or_run_args(cx, &mut cargo);
            if term::verbose() {
                status!("Running", "{cargo}");
                cargo.stdout_to_stderr().run()?;
            } else {
                // Capture output to prevent duplicate warnings from appearing in two runs.
                cargo.run_with_output()?;
            }
        }

        cargo.arg("run");
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        if let Err(e) = cargo.run() {
            warn!("{e:#}");
        }
    } else {
        cargo.arg("run");
        cargo::test_or_run_args(cx, &mut cargo);
        if term::verbose() {
            status!("Running", "{cargo}");
        }
        stdout_to_stderr(cx, &mut cargo);
        cargo.run()?;
    }
    Ok(())
}

fn stdout_to_stderr(cx: &Context, cargo: &mut ProcessBuilder) {
    if cx.args.report.no_report
        || cx.args.report.output_dir.is_some()
        || cx.args.report.output_path.is_some()
    {
        // Do not redirect if unnecessary.
    } else {
        // Redirect stdout to stderr as the report is output to stdout by default.
        cargo.stdout_to_stderr();
    }
}

fn target_u_lower(target: &str) -> String {
    target.replace(['-', '.'], "_")
}
fn target_u_upper(target: &str) -> String {
    let mut target = target_u_lower(target);
    target.make_ascii_uppercase();
    target
}

fn os_str_to_str(s: &OsStr) -> Result<&str> {
    s.to_str().with_context(|| {
        #[allow(clippy::unnecessary_debug_formatting)]
        {
            format!("{} ({s:?}) contains invalid utf-8 data", s.display())
        }
    })
}
