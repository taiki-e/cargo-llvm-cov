// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use camino::Utf8PathBuf;

use crate::{
    cargo::Workspace,
    cli::{self, Args, Subcommand, UnresolvedArgs},
    env,
    metadata::{Package, PackageId},
    process::ProcessBuilder,
    term,
};

pub(crate) struct Context {
    pub(crate) ws: Workspace,

    pub(crate) args: Args,

    pub(crate) workspace_members: WorkspaceMembers,
    pub(crate) current_dir: PathBuf,

    // Paths to executables.
    pub(crate) current_exe: PathBuf,
    /// Path to llvm-cov, can be overridden with `LLVM_COV` environment variable.
    pub(crate) llvm_cov: PathBuf,
    /// Path to llvm-profdata, can be overridden with `LLVM_PROFDATA` environment variable.
    pub(crate) llvm_profdata: PathBuf,

    /// `LLVM_COV_FLAGS` environment variable to pass additional flags to llvm-cov.
    /// (value: space-separated list)
    pub(crate) llvm_cov_flags: Option<String>,
    /// `LLVM_PROFDATA_FLAGS` environment variable to pass additional flags to llvm-profdata.
    /// (value: space-separated list)
    pub(crate) llvm_profdata_flags: Option<String>,

    /// Whether `-Z doctest-in-workspace` is needed.
    pub(crate) need_doctest_in_workspace: bool,
    /// Whether `-C instrument-coverage` is available.
    pub(crate) stable_coverage: bool,
}

impl Context {
    pub(crate) fn new((mut args, unresolved_args): (Args, UnresolvedArgs)) -> Result<Self> {
        let show_env = args.subcommand == Subcommand::ShowEnv;
        let mut ws = Workspace::new(
            unresolved_args.manifest_path.as_deref(),
            args.target.as_deref(),
            show_env,
        )?;
        cli::merge_config_and_args(
            &mut ws,
            &mut args.target,
            &mut args.verbose,
            unresolved_args.color,
        )?;
        term::set_coloring(&mut ws.config.term.color);
        term::verbose::set(args.verbose != 0);

        if !matches!(args.subcommand, Subcommand::Report { .. } | Subcommand::Clean)
            && (!args.build.no_cfg_coverage
                || ws.rustc_version.nightly && !args.build.no_cfg_coverage_nightly)
        {
            let mut cfgs = String::new();
            let mut flags = String::new();
            if !args.build.no_cfg_coverage {
                cfgs.push_str("cfg(coverage)");
                flags.push_str("--no-cfg-coverage");
            }
            if ws.rustc_version.nightly && !args.build.no_cfg_coverage_nightly {
                if cfgs.is_empty() {
                    cfgs.push_str("cfg(coverage_nightly)");
                    flags.push_str("--no-cfg-coverage-nightly");
                } else {
                    cfgs.push_str(" and cfg(coverage_nightly)");
                    flags.push_str(" and --no-cfg-coverage-nightly");
                }
            }
            info!("cargo-llvm-cov currently setting {cfgs}; you can opt-out it by passing {flags}");
        }
        if args.report.output_dir.is_none() && args.report.html {
            args.report.output_dir = Some(ws.default_output_dir.clone());
        }
        if !matches!(args.subcommand, Subcommand::Report { .. } | Subcommand::Clean)
            && env::var_os("CARGO_LLVM_COV_SHOW_ENV").is_some()
        {
            if args.subcommand == Subcommand::ShowEnv {
                warn!("nested show-env may not work correctly");
            } else {
                warn!(
                    "cargo-llvm-cov subcommands other than report and clean may not work correctly \
                     in context where environment variables are set by show-env; consider using \
                     normal {} commands",
                    if args.subcommand.call_cargo_nextest() { "cargo-nextest" } else { "cargo" }
                );
            }
        }
        if ws.config.build.build_dir.is_some()
            && matches!(
                args.subcommand,
                Subcommand::Nextest { archive_file: true } | Subcommand::NextestArchive
            )
        {
            warn!("nextest archive may not work with Cargo build-dir");
        }

        let (llvm_cov, llvm_profdata): (PathBuf, PathBuf) = match (
            env::var_os("LLVM_COV").map(PathBuf::from),
            env::var_os("LLVM_PROFDATA").map(PathBuf::from),
        ) {
            (Some(llvm_cov), Some(llvm_profdata)) => (llvm_cov, llvm_profdata),
            (llvm_cov_env, llvm_profdata_env) => {
                if llvm_cov_env.is_some() {
                    warn!(
                        "setting only LLVM_COV environment variable may not work properly; consider setting both LLVM_COV and LLVM_PROFDATA environment variables"
                    );
                } else if llvm_profdata_env.is_some() {
                    warn!(
                        "setting only LLVM_PROFDATA environment variable may not work properly; consider setting both LLVM_COV and LLVM_PROFDATA environment variables"
                    );
                }
                // --print target-libdir (without --target flag) returns $sysroot/lib/rustlib/$host_triple/lib
                // llvm-tools exists in $sysroot/lib/rustlib/$host_triple/bin
                // https://github.com/rust-lang/rust/issues/85658
                // https://github.com/rust-lang/rust/blob/1.84.0/src/bootstrap/src/core/build_steps/dist.rs#L454
                let mut rustlib: PathBuf = ws.rustc_print("target-libdir")?.into();
                rustlib.pop(); // lib
                rustlib.push("bin");
                let llvm_cov = rustlib.join(format!("llvm-cov{}", env::consts::EXE_SUFFIX));
                let llvm_profdata =
                    rustlib.join(format!("llvm-profdata{}", env::consts::EXE_SUFFIX));
                // Check if required tools are installed.
                if !llvm_cov.exists() || !llvm_profdata.exists() {
                    let sysroot: Utf8PathBuf = ws.rustc_print("sysroot")?.into();
                    let toolchain = sysroot.file_name().unwrap();
                    if cmd!("rustup", "toolchain", "list")
                        .read()
                        .is_ok_and(|t| t.contains(toolchain))
                    {
                        // If toolchain is installed from rustup and llvm-tools-preview is not installed,
                        // suggest installing llvm-tools-preview via rustup.
                        // Include --toolchain flag because the user may be using toolchain
                        // override shorthand (+toolchain).
                        // Note: In some toolchain versions llvm-tools-preview can also be installed as llvm-tools,
                        // but it is an upstream bug. https://github.com/rust-lang/rust/issues/119164
                        let cmd = cmd!(
                            "rustup",
                            "component",
                            "add",
                            "llvm-tools-preview",
                            "--toolchain",
                            toolchain
                        );
                        let ask = match env::var_os("CARGO_LLVM_COV_SETUP") {
                            None => true,
                            Some(ref v) if v == "yes" => false,
                            Some(v) => {
                                #[allow(clippy::unnecessary_debug_formatting)]
                                if v != "no" {
                                    bail!(
                                        "CARGO_LLVM_COV_SETUP must be yes or no, but found `{v:?}`"
                                    );
                                }
                                bail!(
                                    "failed to find llvm-tools-preview, please install llvm-tools-preview \
                                     with `rustup component add llvm-tools-preview --toolchain {toolchain}`",
                                );
                            }
                        };
                        ask_to_run(
                            &cmd,
                            ask,
                            "install the `llvm-tools-preview` component for the selected toolchain",
                        )?;
                        (
                            llvm_cov_env.unwrap_or(llvm_cov),
                            llvm_profdata_env.unwrap_or(llvm_profdata),
                        )
                    } else if llvm_cov_env.is_none() && llvm_profdata_env.is_none() {
                        match find_distro_tools(&ws) {
                            Some(pair) => pair,
                            None => bail!(
                                "failed to find llvm-tools-preview, please install llvm-tools-preview, or set LLVM_COV and LLVM_PROFDATA environment variables",
                            ),
                        }
                    } else {
                        bail!(
                            "failed to find llvm-tools-preview, please install llvm-tools-preview, or set LLVM_COV and LLVM_PROFDATA environment variables",
                        );
                    }
                } else {
                    (llvm_cov_env.unwrap_or(llvm_cov), llvm_profdata_env.unwrap_or(llvm_profdata))
                }
            }
        };

        let workspace_members = WorkspaceMembers::new(
            &ws,
            &unresolved_args.exclude_from_report,
            &unresolved_args.package,
            args.workspace,
        )?;
        if workspace_members.included.is_empty() {
            bail!("no crates to be measured for coverage");
        }

        let mut llvm_cov_flags = env::var("LLVM_COV_FLAGS")?;
        if llvm_cov_flags.is_none() {
            llvm_cov_flags = env::var("CARGO_LLVM_COV_FLAGS")?;
            if llvm_cov_flags.is_some() {
                warn!("CARGO_LLVM_COV_FLAGS is deprecated; consider using LLVM_COV_FLAGS instead");
            }
        }
        let mut llvm_profdata_flags = env::var("LLVM_PROFDATA_FLAGS")?;
        if llvm_profdata_flags.is_none() {
            llvm_profdata_flags = env::var("CARGO_LLVM_PROFDATA_FLAGS")?;
            if llvm_profdata_flags.is_some() {
                warn!(
                    "CARGO_LLVM_PROFDATA_FLAGS is deprecated; consider using LLVM_PROFDATA_FLAGS instead"
                );
            }
        }

        let mut need_doctest_in_workspace = false;
        if args.doctests && !has_z_flag(&args.build.cargo_args, "doctest-in-workspace") {
            need_doctest_in_workspace = cmd!(ws.config.cargo(), "-Z", "help")
                .read()
                .is_ok_and(|s| s.contains("doctest-in-workspace"));
        }

        if args.doctests && !ws.rustc_version.nightly {
            warn!(
                "--doctests flag requires nightly toolchain; consider using `cargo +nightly llvm-cov`"
            );
        }
        if args.build.branch && !ws.rustc_version.nightly {
            warn!(
                "--branch flag requires nightly toolchain; consider using `cargo +nightly llvm-cov`"
            );
        }
        if args.build.mcdc && !ws.rustc_version.nightly {
            warn!(
                "--mcdc flag requires nightly toolchain; consider using `cargo +nightly llvm-cov`"
            );
        }
        let stable_coverage =
            ws.rustc().args(["-C", "help"]).read()?.contains("instrument-coverage");
        if !stable_coverage && !ws.rustc_version.nightly {
            warn!(
                "cargo-llvm-cov requires rustc 1.60+; consider updating toolchain (`rustup update`)
                 or using nightly toolchain (`cargo +nightly llvm-cov`)"
            );
        }

        Ok(Self {
            ws,
            args,
            workspace_members,
            current_dir: env::current_dir().unwrap(),
            current_exe: match env::current_exe() {
                Ok(exe) => exe,
                Err(e) => {
                    let exe = format!("cargo-llvm-cov{}", env::consts::EXE_SUFFIX);
                    warn!(
                        "failed to get current executable, assuming {exe} in PATH as current executable: {e}"
                    );
                    exe.into()
                }
            },
            llvm_cov,
            llvm_profdata,
            llvm_cov_flags,
            llvm_profdata_flags,
            stable_coverage,
            need_doctest_in_workspace,
        })
    }

    pub(crate) fn process(&self, program: impl Into<OsString>) -> ProcessBuilder {
        let mut cmd = cmd!(program);
        // cargo displays env vars only with -vv.
        if self.args.verbose > 1 {
            cmd.display_env_vars();
        }
        cmd
    }

    pub(crate) fn cargo(&self) -> ProcessBuilder {
        self.ws.cargo(self.args.verbose)
    }
}

pub(crate) struct WorkspaceMembers {
    pub(crate) excluded: Vec<PackageId>,
    pub(crate) included: Vec<PackageId>,
}

impl WorkspaceMembers {
    fn new(
        ws: &Workspace,
        exclude_from_report: &[String],
        package: &[String],
        workspace: bool,
    ) -> Result<Self> {
        let mut excluded = vec![];
        let mut included = vec![];
        // Refs: https://github.com/rust-lang/cargo/blob/0d08b955e5f6171f81e5268b91a7d70f2e94b62f/src/cargo/ops/cargo_compile/packages.rs
        let mut opt_out = if exclude_from_report.is_empty() {
            None
        } else {
            Some(find_ids(ws, exclude_from_report)?)
        };
        if opt_out.is_none() && workspace {
            included.extend_from_slice(&ws.metadata.workspace_members);
        } else {
            let mut opt_in = if package.is_empty() { None } else { Some(find_ids(ws, package)?) };
            'outer: for &id in &ws.metadata.workspace_members {
                if let Some((ids, pats)) = &mut opt_out {
                    // --exclude
                    if ids.contains(&id) {
                        excluded.push(id);
                        continue;
                    }
                    let name = &ws.metadata[id].name;
                    for pat in pats {
                        if pat.0.matches(name) {
                            excluded.push(id);
                            pat.1 = true;
                            continue 'outer;
                        }
                    }
                }
                if workspace {
                    // --workspace
                    included.push(id);
                } else if let Some((ids, pats)) = &mut opt_in {
                    // --package
                    if ids.contains(&id) {
                        included.push(id);
                        continue;
                    }
                    let name = &ws.metadata[id].name;
                    for pat in pats {
                        if pat.0.matches(name) {
                            included.push(id);
                            pat.1 = true;
                            continue 'outer;
                        }
                    }
                    excluded.push(id);
                } else if let Some(current_package) = ws.current_package {
                    // root of non-virtual workspace or member of virtual workspace
                    if id == current_package {
                        included.push(id);
                    } else {
                        excluded.push(id);
                    }
                } else {
                    // root of virtual workspace
                    included.push(id);
                }
            }

            if let Some((_, pats)) = &opt_out {
                for (pat, matched) in pats {
                    if !matched {
                        warn!("not found package pattern '{pat}' in workspace");
                    }
                }
            }
            if let Some((_, pats)) = &opt_in {
                for (pat, matched) in pats {
                    if !matched {
                        warn!("not found package pattern '{pat}' in workspace");
                    }
                }
            }
        }

        Ok(Self { excluded, included })
    }
}

fn find_ids(
    ws: &Workspace,
    list: &[String],
) -> Result<(HashSet<PackageId>, Vec<(glob::Pattern, bool)>)> {
    let mut ids = HashSet::with_capacity(list.len());
    let mut patterns = vec![];
    for e in list {
        let mut found = false;
        for &id in &ws.metadata.workspace_members {
            if match_pkg_spec(&ws.metadata[id], e)? {
                ids.insert(id);
                found = true;
                break;
            }
        }
        if !found {
            if e.contains(['*', '?', '[', ']']) {
                patterns.push((
                    glob::Pattern::new(e)
                        .with_context(|| format!("cannot build glob pattern from `{e}`"))?,
                    false,
                ));
            } else {
                warn!("not found package '{e}' in workspace");
            }
        }
    }
    Ok((ids, patterns))
}

fn match_pkg_spec(pkg: &Package, name_or_spec: &str) -> Result<bool> {
    /*
    Refs: https://doc.rust-lang.org/1.93.0/cargo/reference/pkgid-spec.html
        spec := pkgname |
            [ kind "+" ] proto "://" hostname-and-path [ "?" query] [ "#" ( pkgname | semver ) ]
        query = ( "branch" | "tag" | "rev" ) "=" ref
        pkgname := name [ ("@" | ":" ) semver ]
        semver := digits [ "." digits [ "." digits [ "-" prerelease ] [ "+" build ]]]

        kind = "registry" | "git" | "path"
        proto := "http" | "git" | "file" | ...
    */
    fn split_spec(s: &str) -> Option<(&str, &str, Option<&str>, Option<&str>)> {
        let (proto_etc, hostname_and_path_etc) = s.split_once("://")?;
        let proto = proto_etc.split_once('+').unwrap_or(("", proto_etc)).1; // drop kind
        let (hostname_and_path_etc, pkgname_or_semver) =
            hostname_and_path_etc.split_once('#').unwrap_or((hostname_and_path_etc, ""));
        let (hostname_and_path, query) =
            hostname_and_path_etc.split_once('?').unwrap_or((hostname_and_path_etc, ""));
        Some((
            proto,
            hostname_and_path,
            if query.is_empty() { None } else { Some(query) },
            if pkgname_or_semver.is_empty() { None } else { Some(pkgname_or_semver) },
        ))
    }
    fn split_semver(
        s: &str,
    ) -> Option<(&str, Option<(&str, Option<(&str, Option<&str>, Option<&str>)>)>)> {
        let mut digits = s.splitn(3, '.');
        let major = digits.next()?;
        let Some(minor) = digits.next() else {
            return Some((major, None));
        };
        let Some(patch_etc) = digits.next() else {
            return Some((major, Some((minor, None))));
        };
        let (patch_etc, meta) = patch_etc.split_once('+').unwrap_or((patch_etc, ""));
        let (patch, pre) = patch_etc.split_once('-').unwrap_or((patch_etc, ""));
        Some((
            major,
            Some((
                minor,
                Some((
                    patch,
                    if pre.is_empty() { None } else { Some(pre) },
                    if meta.is_empty() { None } else { Some(meta) },
                )),
            )),
        ))
    }
    let name = &*pkg.name;
    let p = name_or_spec;
    let (version, full_version) = if p.starts_with(name) {
        if p.len() == name.len() {
            return Ok(true); // version omitted
        }
        if !matches!(p.as_bytes().get(name.len()), Some(&b'@' | &b':')) {
            return Ok(false); // pkgname unmatched
        }
        (&p[name.len() + 1..], &*pkg.version)
    } else {
        let p = p.trim_ascii_end(); // pkgid may contains trailing newline (e.g., when pkgid is got from `cargo pkgid -p <package>`)
        let full = &*pkg.id;
        let Some((proto, hostname_and_path, query, pkgname_or_semver)) = split_spec(p) else {
            return Ok(false); // p is not pkg spec
        };
        let Some((full_proto, full_hostname_and_path, full_query, full_pkgname_or_semver)) =
            split_spec(full)
        else {
            bail!("invalid pkg spec ({full}) from cargo-metadata")
        };
        if proto != full_proto || hostname_and_path != full_hostname_and_path {
            return Ok(false); // proto or hostname-and-path unmatched
        }
        if query.is_some() && query != full_query {
            return Ok(false); // query unmatched
        }
        let Some(pkgname_or_semver) = pkgname_or_semver else {
            return Ok(true); // pkgname | semver omitted
        };
        let Some(full_pkgname_or_semver) = full_pkgname_or_semver else {
            return Ok(false); // extra pkgname | semver
        };
        match (
            pkgname_or_semver.split_once(['@', ':']),
            full_pkgname_or_semver.split_once(['@', ':']),
        ) {
            (Some((pkgname, semver)), Some((full_pkgname, full_semver))) => {
                if pkgname != full_pkgname {
                    return Ok(false); // pkgname unmatched
                }
                (semver, full_semver)
            }
            (Some(_), None) => return Ok(false), // extra semver
            (None, _) => return Ok(true),        // pkgname omitted or no pkgname in spec
        }
    };
    let Some((major, minor_etc)) = split_semver(version) else {
        warn!("invalid pkg version ({version}) from --package");
        return Ok(false); // invalid version
    };
    let Some((full_major, Some((full_minor, Some((full_patch, full_pre, full_meta)))))) =
        split_semver(full_version)
    else {
        bail!("invalid pkg version ({full_version}) from cargo-metadata")
    };
    if major != full_major {
        return Ok(false); // major unmatched
    }
    let Some((minor, patch_etc)) = minor_etc else {
        return Ok(true); // minor version omitted
    };
    if minor != full_minor {
        return Ok(false); // minor unmatched
    }
    let Some((patch, pre, meta)) = patch_etc else {
        return Ok(true); // patch version omitted
    };
    if patch != full_patch
        || pre.is_some() && pre != full_pre
        || meta.is_some() && meta != full_meta
    {
        return Ok(false); // patch or pre or meta unmatched
    }
    Ok(true)
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

// Adapted from https://github.com/rust-lang/miri/blob/dba35d2be72f4b78343d1a0f0b4737306f310672/cargo-miri/src/util.rs#L181-L204
fn ask_to_run(cmd: &ProcessBuilder, ask: bool, text: &str) -> Result<()> {
    // Disable interactive prompts in CI (GitHub Actions, Travis, AppVeyor, etc).
    // Azure doesn't set `CI` though (nothing to see here, just Microsoft being Microsoft),
    // so we also check their `TF_BUILD`.
    let is_ci = env::var_os("CI").is_some() || env::var_os("TF_BUILD").is_some();
    if ask && !is_ci {
        let mut buf = String::new();
        eprint!("I will run {cmd} to {text}.\nProceed? [Y/n] ");
        io::stderr().flush()?;
        io::stdin().read_line(&mut buf)?;
        match buf.trim().to_lowercase().as_str() {
            // Proceed.
            "" | "y" | "yes" => {}
            "n" | "no" => bail!("aborting as per your request"),
            a => bail!("invalid answer `{a}`"),
        }
    } else {
        info!("running {} to {}", cmd, text);
    }

    cmd.run()?;
    Ok(())
}

fn find_in_path(
    name: &str,
    path: Option<&OsStr>,
    mut get_version: impl FnMut(&Path) -> Option<String>,
) -> Option<(PathBuf, String)> {
    let path = path?;
    let file_name = format!("{name}{}", env::consts::EXE_SUFFIX);
    for dir in env::split_paths(path) {
        let candidate = dir.join(&file_name);
        if candidate.is_file() {
            if let Some(version) = get_version(&candidate) {
                return Some((candidate, version));
            }
        }
    }
    None
}

fn find_pair_in_path_from(
    path: Option<&OsStr>,
    mut get_version: impl FnMut(&str, &Path) -> Option<String>,
) -> Option<((PathBuf, String), (PathBuf, String))> {
    let llvm_cov = find_in_path("llvm-cov", path, |p| get_version("llvm-cov", p))?;
    let llvm_profdata = find_in_path("llvm-profdata", path, |p| get_version("llvm-profdata", p))?;
    Some((llvm_cov, llvm_profdata))
}

fn find_pair_in_path() -> Option<((PathBuf, String), (PathBuf, String))> {
    find_pair_in_path_from(env::var_os("PATH").as_deref(), |_, p| llvm_tool_version(p))
}

fn parse_rustc_llvm_version(output: &str) -> Option<&str> {
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("LLVM version:") {
            let version = rest.split_whitespace().next()?;
            if !version.is_empty() {
                return Some(version);
            }
        }
    }
    None
}

fn parse_llvm_tool_version(output: &str) -> Option<&str> {
    for line in output.lines() {
        let line = line.trim();
        if let Some(idx) = line.find("LLVM version") {
            let mut rest = line[idx + "LLVM version".len()..].trim();
            if let Some(stripped) = rest.strip_prefix(':') {
                rest = stripped.trim();
            }
            let version = rest.split_whitespace().next()?;
            if !version.is_empty() {
                return Some(version);
            }
        }
    }
    None
}

fn versions_match(rustc: &str, cov: &str, profdata: &str) -> bool {
    rustc == cov && cov == profdata
}

fn rustc_llvm_version(ws: &Workspace) -> Option<String> {
    let output = ws.rustc().args(["-vV"]).read().ok()?;
    parse_rustc_llvm_version(&output).map(ToOwned::to_owned)
}

fn llvm_tool_version(path: &Path) -> Option<String> {
    let output = cmd!(path, "--version").read().ok()?;
    parse_llvm_tool_version(&output).map(ToOwned::to_owned)
}

fn find_distro_tools(ws: &Workspace) -> Option<(PathBuf, PathBuf)> {
    let Some(((llvm_cov, cov_version), (llvm_profdata, profdata_version))) = find_pair_in_path()
    else {
        if term::verbose() {
            info!(
                "fallback to PATH LLVM tools rejected: llvm-cov or llvm-profdata not found in PATH"
            );
        }
        return None;
    };

    let Some(rustc_version) = rustc_llvm_version(ws) else {
        if term::verbose() {
            info!("fallback to PATH LLVM tools rejected: failed to determine rustc LLVM version");
        }
        return None;
    };

    if versions_match(&rustc_version, &cov_version, &profdata_version) {
        Some((llvm_cov, llvm_profdata))
    } else {
        if term::verbose() {
            info!(
                "fallback to PATH LLVM tools rejected: rustc LLVM version ({rustc_version}) does not match llvm-cov ({cov_version}) and llvm-profdata ({profdata_version})"
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        Package, find_in_path, find_pair_in_path_from, match_pkg_spec, parse_llvm_tool_version,
        parse_rustc_llvm_version, versions_match,
    };

    #[test]
    fn test_match_pkg_spec() {
        // Examples are from https://doc.rust-lang.org/1.93.0/cargo/reference/pkgid-spec.html#example-specifications

        // crates.io
        let pkg = &Package {
            id: "registry+https://github.com/rust-lang/crates.io-index#regex@1.4.3".into(),
            name: "regex".into(),
            version: "1.4.3".into(),
            targets: Box::default(),
            manifest_path: "".into(),
        };
        // name
        assert!(match_pkg_spec(pkg, "regex").unwrap());
        assert!(!match_pkg_spec(pkg, "regex-syntax").unwrap());
        // name+version
        assert!(match_pkg_spec(pkg, "regex@1").unwrap());
        assert!(match_pkg_spec(pkg, "regex@1.4").unwrap());
        assert!(match_pkg_spec(pkg, "regex@1.4.3").unwrap());
        assert!(match_pkg_spec(pkg, "regex:1.4").unwrap());
        assert!(!match_pkg_spec(pkg, "regex@2").unwrap());
        assert!(!match_pkg_spec(pkg, "regex@1.5").unwrap());
        assert!(!match_pkg_spec(pkg, "regex@1.4.2").unwrap());
        assert!(!match_pkg_spec(pkg, "regex@1.4.4").unwrap());
        // spec
        assert!(match_pkg_spec(pkg, "https://github.com/rust-lang/crates.io-index#regex").unwrap());
        assert!(
            match_pkg_spec(pkg, "https://github.com/rust-lang/crates.io-index#regex@1.4.3")
                .unwrap()
        );
        assert!(
            match_pkg_spec(pkg, "https://github.com/rust-lang/crates.io-index#regex@1.4").unwrap()
        );
        assert!(
            match_pkg_spec(
                pkg,
                "registry+https://github.com/rust-lang/crates.io-index#regex@1.4.3"
            )
            .unwrap()
        );

        // git
        let pkg = &Package {
            id: "git+ssh://git@github.com/rust-lang/regex.git?branch=dev#regex@1.4.3".into(),
            name: "regex".into(),
            version: "1.4.3".into(),
            targets: Box::default(),
            manifest_path: "".into(),
        };
        assert!(match_pkg_spec(pkg, "regex").unwrap());
        assert!(
            match_pkg_spec(pkg, "ssh://git@github.com/rust-lang/regex.git#regex@1.4.3").unwrap()
        );
        assert!(
            match_pkg_spec(pkg, "git+ssh://git@github.com/rust-lang/regex.git#regex@1.4.3")
                .unwrap()
        );
        assert!(
            match_pkg_spec(
                pkg,
                "git+ssh://git@github.com/rust-lang/regex.git?branch=dev#regex@1.4.3"
            )
            .unwrap()
        );
        let pkg = &Package {
            id: "git+https://github.com/rust-lang/cargo#0.52.0".into(),
            name: "cargo".into(),
            version: "0.52.0".into(),
            targets: Box::default(),
            manifest_path: "".into(),
        };
        assert!(match_pkg_spec(pkg, "https://github.com/rust-lang/cargo#0.52.0").unwrap());
        assert!(match_pkg_spec(pkg, "git+https://github.com/rust-lang/cargo#0.52.0").unwrap());
        assert!(
            !match_pkg_spec(pkg, "https://github.com/rust-lang/cargo#cargo-platform@0.1.2")
                .unwrap()
        );

        // local
        let pkg = &Package {
            id: "path+file:///path/to/my/project/foo#1.1.8".into(),
            name: "foo".into(),
            version: "1.1.8".into(),
            targets: Box::default(),
            manifest_path: "".into(),
        };
        assert!(match_pkg_spec(pkg, "foo").unwrap());
        assert!(match_pkg_spec(pkg, "file:///path/to/my/project/foo").unwrap());
        assert!(match_pkg_spec(pkg, "file:///path/to/my/project/foo#1.1.8").unwrap());
        assert!(match_pkg_spec(pkg, "path+file:///path/to/my/project/foo#1.1").unwrap());
        assert!(match_pkg_spec(pkg, "path+file:///path/to/my/project/foo#1.1.8").unwrap());
    }

    #[test]
    fn test_parse_rustc_llvm_version() {
        let realistic = "\
rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux rust 1:1.98.1-1.1)
binary: rustc
commit-hash: 48a229ceaefd4985c50990b14116b6d856af0985
commit-date: 2026-09-01
host: x86_64-unknown-linux-gnu
release: 1.98.1
LLVM version: 22.1.8
";
        assert_eq!(parse_rustc_llvm_version(realistic), Some("22.1.8"));

        let with_trailing = "\
release: 1.98.1
LLVM version: 22.1.8-rust-1.80.0
";
        assert_eq!(parse_rustc_llvm_version(with_trailing), Some("22.1.8-rust-1.80.0"));

        let missing = "\
rustc 1.98.1
binary: rustc
release: 1.98.1
";
        assert_eq!(parse_rustc_llvm_version(missing), None);

        // Malformed cases
        assert_eq!(parse_rustc_llvm_version("LLVM version:"), None);
        assert_eq!(parse_rustc_llvm_version("LLVM version:   "), None);
        assert_eq!(parse_rustc_llvm_version(""), None);
        assert_eq!(parse_rustc_llvm_version("random text without llvm version"), None);
    }

    #[test]
    fn test_parse_llvm_tool_version() {
        let realistic = "\
LLVM (http://llvm.org/):
  LLVM version 22.1.8
  Optimized build.
";
        assert_eq!(parse_llvm_tool_version(realistic), Some("22.1.8"));

        let distro_prefix = "\
Ubuntu LLVM version 18.1.8
  Optimized build.
";
        assert_eq!(parse_llvm_tool_version(distro_prefix), Some("18.1.8"));

        let homebrew = "\
Homebrew LLVM version 19.1.3
  Optimized build.
";
        assert_eq!(parse_llvm_tool_version(homebrew), Some("19.1.3"));

        let with_colon = "\
LLVM version: 22.1.8
";
        assert_eq!(parse_llvm_tool_version(with_colon), Some("22.1.8"));

        let missing = "\
LLVM (http://llvm.org/):
  Optimized build.
";
        assert_eq!(parse_llvm_tool_version(missing), None);

        // Malformed cases
        assert_eq!(parse_llvm_tool_version("LLVM version"), None);
        assert_eq!(parse_llvm_tool_version("LLVM version:"), None);
        assert_eq!(parse_llvm_tool_version("LLVM version   "), None);
        assert_eq!(parse_llvm_tool_version(""), None);
        assert_eq!(parse_llvm_tool_version("garbage output"), None);
    }

    #[test]
    fn test_versions_match() {
        // Exact version match accepted
        assert!(versions_match("22.1.8", "22.1.8", "22.1.8"));

        // Patch mismatch rejected
        assert!(!versions_match("22.1.8", "22.1.7", "22.1.8"));
        assert!(!versions_match("22.1.8", "22.1.8", "22.1.7"));
        assert!(!versions_match("22.1.7", "22.1.8", "22.1.8"));

        // Minor mismatch rejected
        assert!(!versions_match("22.1.8", "22.0.0", "22.0.0"));

        // Major mismatch rejected
        assert!(!versions_match("22.1.8", "21.1.8", "21.1.8"));
        assert!(!versions_match("21.1.8", "22.1.8", "22.1.8"));
    }

    #[test]
    fn test_find_in_path() {
        let temp_dir1 = tempfile::tempdir().unwrap();
        let temp_dir2 = tempfile::tempdir().unwrap();
        let cov_name = format!("llvm-cov{}", std::env::consts::EXE_SUFFIX);

        let path_env = std::env::join_paths([temp_dir1.path(), temp_dir2.path()]).unwrap();

        // 1. Neither found
        assert_eq!(find_in_path("llvm-cov", Some(&path_env), |_| None), None);

        // 2. Earlier candidate in PATH is unusable (e.g. non-executable 0644), later candidate is valid
        fs_err::write(temp_dir1.path().join(&cov_name), b"not executable").unwrap();
        fs_err::write(temp_dir2.path().join(&cov_name), b"valid").unwrap();

        let result = find_in_path("llvm-cov", Some(&path_env), |path| {
            if path == temp_dir2.path().join(&cov_name) { Some("22.1.8".to_owned()) } else { None }
        });
        assert_eq!(result, Some((temp_dir2.path().join(&cov_name), "22.1.8".to_owned())));

        // 3. Edge case: path is None
        assert_eq!(find_in_path("llvm-cov", None, |_| Some("22.1.8".to_owned())), None);
    }

    #[test]
    fn test_find_pair_in_path() {
        let temp_dir1 = tempfile::tempdir().unwrap();
        let temp_dir2 = tempfile::tempdir().unwrap();
        let cov_name = format!("llvm-cov{}", std::env::consts::EXE_SUFFIX);
        let profdata_name = format!("llvm-profdata{}", std::env::consts::EXE_SUFFIX);

        let path_env = std::env::join_paths([temp_dir1.path(), temp_dir2.path()]).unwrap();

        let mock_version = |_name: &str, _path: &Path| Some("22.1.8".to_owned());

        // 1. Neither found
        assert_eq!(find_pair_in_path_from(Some(&path_env), mock_version), None);

        // 2. Only llvm-cov found
        fs_err::write(temp_dir1.path().join(&cov_name), b"").unwrap();
        assert_eq!(find_pair_in_path_from(Some(&path_env), mock_version), None);

        // 3. Only llvm-profdata found
        fs_err::remove_file(temp_dir1.path().join(&cov_name)).unwrap();
        fs_err::write(temp_dir2.path().join(&profdata_name), b"").unwrap();
        assert_eq!(find_pair_in_path_from(Some(&path_env), mock_version), None);

        // 4. Both found (even across different directories in PATH)
        fs_err::write(temp_dir1.path().join(&cov_name), b"").unwrap();
        let (cov, profdata) = find_pair_in_path_from(Some(&path_env), mock_version).unwrap();
        assert_eq!(cov, (temp_dir1.path().join(&cov_name), "22.1.8".to_owned()));
        assert_eq!(profdata, (temp_dir2.path().join(&profdata_name), "22.1.8".to_owned()));

        // Edge case: path is None
        assert_eq!(find_pair_in_path_from(None, mock_version), None);
    }
}
