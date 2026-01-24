// SPDX-License-Identifier: Apache-2.0 OR MIT

// RUSTC_WRAPPER mode for internal use.
// Do NOT use this directly since this is an unstable interface.

use std::ffi::OsString;

use anyhow::{Context as _, Result};
use cargo_config2::Flags;
use lexopt::Arg::{Long, Short, Value};

use crate::{EnvTarget, cli, context::Context, env, process::ProcessBuilder};

const ENV_ENABLED: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER";
const ENV_RUSTFLAGS: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS";
const ENV_COVERAGE_TARGET: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_COVERAGE_TARGET";
const ENV_HOST: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_HOST";
const ENV_CRATE_NAMES: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_CRATE_NAMES";
const ENV_PRE_EXISTING: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_PRE_EXISTING";

// -----------------------------------------------------------------------------
// For caller

pub(crate) fn use_wrapper(cx: &Context) -> bool {
    if cx.args.no_rustc_wrapper {
        // Explicitly disabled.
        return false;
    }
    true
}

pub(crate) fn set_env(cx: &Context, env: &mut dyn EnvTarget, rustflags: &Flags) -> Result<()> {
    if !use_wrapper(cx) {
        // Handle nested calls.
        if env::var_os(ENV_ENABLED).is_some() {
            if let Some(pre_existing_wrapper) = env::var_os(ENV_PRE_EXISTING) {
                env.set_os("RUSTC_WRAPPER", &pre_existing_wrapper)?;
            } else {
                env.unset("RUSTC_WRAPPER")?;
            }
        }
        return Ok(());
    }

    env.set(ENV_ENABLED, "1")?;
    env.set(ENV_RUSTFLAGS, &rustflags.encode()?)?;
    match (cx.args.coverage_target_only, &cx.args.target) {
        (true, Some(coverage_target)) => {
            env.set(ENV_COVERAGE_TARGET, coverage_target)?;
            env.set(ENV_HOST, cx.ws.config.host_triple()?)?;
        }
        _ => {
            env.unset(ENV_COVERAGE_TARGET)?;
            env.unset(ENV_HOST)?;
        }
    }
    let mut crates = String::new();
    for &id in &cx.ws.metadata.workspace_members {
        let pkg = &cx.ws.metadata[id];
        let name = &pkg.name.replace('-', "_");
        crates.push_str(name);
        crates.push(',');
        crates.push_str(name);
        crates.push_str("_tests,"); // for try_build
        for target in &pkg.targets {
            crates.push_str(&target.name.replace('-', "_"));
            crates.push(',');
        }
    }
    for dep in &cx.args.cov.dep_coverage {
        let name = &dep.replace('-', "_");
        // TODO: should refer the lib name.
        crates.push_str(name);
        crates.push(',');
    }
    crates.pop(); // drop trailing coma
    env.set(ENV_CRATE_NAMES, &crates)?;
    env.set_os("RUSTC_WRAPPER", cx.current_exe.as_os_str())?;
    if let Some(pre_existing_wrapper) = cx.ws.config.build.rustc_wrapper.as_deref() {
        env.set_os(ENV_PRE_EXISTING, pre_existing_wrapper.as_os_str())?;
    } else {
        env.unset(ENV_PRE_EXISTING)?;
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// For callee

pub(crate) fn is_enabled() -> bool {
    if env::var_os(ENV_ENABLED).is_none() {
        return false;
    }
    let mut args = env::args_os();
    args.next(); // cargo or cargo-llvm-cov
    let Some(first) = args.next() else { return false };
    if first == cli::FIRST_SUBCMD {
        // Handle nested calls.
        // In show-env context, CARGO_LLVM_COV_RUSTC_WRAPPER may be set for
        // other subcommands. For example, a situation where a process to
        // generate a report is expected despite CARGO_LLVM_COV_RUSTC_WRAPPER
        // being set may occur.
        return false;
    }
    true
}

pub(crate) fn try_main() -> Result<()> {
    debug_assert!(is_enabled());

    // Parse arguments.
    let mut raw_args = env::args_os();
    raw_args.next(); // cargo-llvm-cov
    let rustc_or_wrapper = if let Some(pre_existing_wrapper) = env::var_os(ENV_PRE_EXISTING) {
        // pre-existing rustc-wrapper
        pre_existing_wrapper
    } else {
        // rustc or rustc-workspace-wrapper
        raw_args.next().context("invalid arguments for rustc-wrapper")?
    };
    let args = raw_args.collect::<Vec<_>>();
    let mut crate_name = None;
    let mut target = None;
    let mut parser = lexopt::Parser::from_args(&args);
    while let Some(arg) = parser.next()? {
        match arg {
            Long("crate-name") => crate_name = Some(parser.value()?),
            Long("target") => target = Some(parser.value()?),
            Long(_) | Short(_) => {
                parser.optional_value();
            }
            Value(_) => {}
        }
    }

    // Skip cases where no crate name specified, e.g., --version, --print.
    let Some(crate_name) = crate_name.filter(|name| name != "___") else {
        return run_rustc_wrapper(rustc_or_wrapper, args, vec![]);
    };

    // Fetch context from env vars.
    let crate_names = env::var_required(ENV_CRATE_NAMES)?;
    let crate_names = crate_names.split(',').collect::<Vec<_>>();
    let wrapper_rustflags = Flags::from_encoded(&env::var_required(ENV_RUSTFLAGS)?).flags;
    let coverage_target = env::var_os(ENV_COVERAGE_TARGET);
    let host = if coverage_target.is_some() { Some(env::var_os_required(ENV_HOST)?) } else { None };

    let apply_wrapper_rustflags = crate_names.iter().any(|&name| name == crate_name)
        && coverage_target
            .is_none_or(|coverage_target| coverage_target == target.unwrap_or(host.unwrap()));

    run_rustc_wrapper(
        rustc_or_wrapper,
        args,
        if apply_wrapper_rustflags { wrapper_rustflags } else { vec![] },
    )
}

fn run_rustc_wrapper(
    rustc_or_wrapper: OsString,
    args: Vec<OsString>,
    wrapper_rustflags: Vec<String>,
) -> Result<()> {
    let mut cmd = ProcessBuilder::new(rustc_or_wrapper);
    cmd.reserve_exact_args(args.len() + wrapper_rustflags.len());
    cmd.args(args);
    cmd.args(wrapper_rustflags);
    cmd.run()?;
    Ok(())
}
