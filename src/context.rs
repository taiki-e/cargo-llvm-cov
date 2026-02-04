// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    collections::HashSet,
    ffi::OsString,
    io::{self, Write as _},
    path::PathBuf,
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
}

impl Context {
    pub(crate) fn new((mut args, unresolved_args): (Args, UnresolvedArgs)) -> Result<Self> {
        let show_env = args.subcommand == Subcommand::ShowEnv;
        let mut ws = Workspace::new(
            unresolved_args.manifest_path.as_deref(),
            args.target.as_deref(),
            args.doctests,
            args.branch,
            args.mcdc,
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

        if args.report.output_dir.is_some() && !args.report.show() {
            // If the format flag is not specified, this flag is no-op.
            args.report.output_dir = None;
        }
        {
            // The following warnings should not be promoted to an error.
            let _guard = term::warn::ignore();
            if args.branch {
                warn!("--branch option is unstable");
            }
            if args.mcdc {
                warn!("--mcdc option is unstable");
            }
            if args.doc {
                warn!("--doc option is unstable");
            } else if args.doctests {
                warn!("--doctests option is unstable");
            }
        }
        if args.coverage_target_only {
            info!(
                "when --coverage-target-only flag is used, coverage for proc-macro and build script will \
                 not be displayed"
            );
        } else if args.no_rustc_wrapper && args.target.is_some() {
            info!(
                "When both --no-rustc-wrapper flag and --target option are used, coverage for proc-macro and \
                 build script will not be displayed because cargo does not pass RUSTFLAGS to them"
            );
        }
        if args.no_rustc_wrapper && !args.dep_coverage.is_empty() {
            warn!("--dep-coverage may not work together with --no-rustc-wrapper");
        }
        if !matches!(args.subcommand, Subcommand::Report { .. } | Subcommand::Clean)
            && (!args.no_cfg_coverage || ws.rustc_version.nightly && !args.no_cfg_coverage_nightly)
        {
            let mut cfgs = String::new();
            let mut flags = String::new();
            if !args.no_cfg_coverage {
                cfgs.push_str("cfg(coverage)");
                flags.push_str("--no-cfg-coverage");
            }
            if ws.rustc_version.nightly && !args.no_cfg_coverage_nightly {
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
                    } else {
                        bail!(
                            "failed to find llvm-tools-preview, please install llvm-tools-preview, or set LLVM_COV and LLVM_PROFDATA environment variables",
                        );
                    }
                }
                (llvm_cov_env.unwrap_or(llvm_cov), llvm_profdata_env.unwrap_or(llvm_profdata))
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

// Adapted from https://github.com/rust-lang/miri/blob/dba35d2be72f4b78343d1a0f0b4737306f310672/cargo-miri/src/util.rs#L181-L204
fn ask_to_run(cmd: &ProcessBuilder, ask: bool, text: &str) -> Result<()> {
    // Disable interactive prompts in CI (GitHub Actions, Travis, AppVeyor, etc).
    // Azure doesn't set `CI` though (nothing to see here, just Microsoft being Microsoft),
    // so we also check their `TF_BUILD`.
    let is_ci = env::var_os("CI").is_some() || env::var_os("TF_BUILD").is_some();
    if ask && !is_ci {
        let mut buf = String::new();
        print!("I will run {cmd} to {text}.\nProceed? [Y/n] ");
        io::stdout().flush()?;
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

#[cfg(test)]
mod tests {
    use super::{Package, match_pkg_spec};

    #[test]
    fn test_match_pkg_spec() {
        // Examples are from https://doc.rust-lang.org/1.93.0/cargo/reference/pkgid-spec.html#example-specifications

        // crates.io
        let pkg = &Package {
            id: "registry+https://github.com/rust-lang/crates.io-index#regex@1.4.3".into(),
            name: "regex".into(),
            version: "1.4.3".into(),
            targets: vec![].into_boxed_slice(),
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
            targets: vec![].into_boxed_slice(),
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
            targets: vec![].into_boxed_slice(),
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
            targets: vec![].into_boxed_slice(),
            manifest_path: "".into(),
        };
        assert!(match_pkg_spec(pkg, "foo").unwrap());
        assert!(match_pkg_spec(pkg, "file:///path/to/my/project/foo").unwrap());
        assert!(match_pkg_spec(pkg, "file:///path/to/my/project/foo#1.1.8").unwrap());
        assert!(match_pkg_spec(pkg, "path+file:///path/to/my/project/foo#1.1").unwrap());
        assert!(match_pkg_spec(pkg, "path+file:///path/to/my/project/foo#1.1.8").unwrap());
    }
}
