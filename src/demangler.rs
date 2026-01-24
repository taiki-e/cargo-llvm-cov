// SPDX-License-Identifier: Apache-2.0 OR MIT

// Demangler mode for internal use.
// Do NOT use this directly since this is an unstable interface.

use std::io::{self, BufWriter, Write as _};

use anyhow::{Result, bail};

use crate::{env, process::ProcessBuilder};

const ENV_ENABLED: &str = "__CARGO_LLVM_COV_DEMANGLER";

// -----------------------------------------------------------------------------
// For caller

pub(crate) fn set_env(cmd: &mut ProcessBuilder) {
    cmd.env(ENV_ENABLED, "1");
}

// -----------------------------------------------------------------------------
// For callee

pub(crate) fn is_enabled() -> bool {
    env::var_os(ENV_ENABLED).is_some()
}

pub(crate) fn try_main() -> Result<()> {
    debug_assert!(is_enabled());

    // Parse arguments.
    let mut args = env::args_os();
    args.next(); // cargo-llvm-cov
    if let Some(arg) = args.next() {
        bail!("invalid arguments for demangler: {}", arg.display())
    }

    // Demangle from stdin to stdout.
    let mut stdout = BufWriter::new(io::stdout().lock()); // Buffered because it is written many times.
    rustc_demangle::demangle_stream(&mut io::stdin().lock(), &mut stdout, false)?;
    stdout.flush()?;
    Ok(())
}
