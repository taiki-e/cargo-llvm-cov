// SPDX-License-Identifier: Apache-2.0 OR MIT

// Refs:
// - https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
// - https://github.com/rust-lang/cargo/blob/0.62.0/src/cargo/ops/cargo_clean.rs

use std::path::Path;

use anyhow::Result;
use camino::Utf8Path;
use walkdir::WalkDir;

use crate::{
    cargo::{self, Workspace},
    cli::{self, Args, CleanOptions, UnresolvedArgs},
    context::Context,
    fs,
    metadata::PackageId,
    regex_vec::{RegexVec, RegexVecBuilder},
    term,
};

pub(crate) fn run((mut args, unresolved_args): (Args, UnresolvedArgs)) -> Result<()> {
    let mut ws =
        Workspace::new(unresolved_args.manifest_path.as_deref(), None, false, false, false, false)?;
    cli::merge_config_and_args(&mut ws, &mut None, &mut args.verbose, unresolved_args.color)?;
    drop(unresolved_args);
    term::set_coloring(&mut ws.config.term.color);

    if !args.workspace && !args.clean.profraw_only {
        for dir in &[&ws.target_dir, &ws.default_output_dir] {
            rm_rf(dir, args.verbose != 0)?;
        }
        if let Some(dir) = &ws.build_dir {
            rm_rf(dir, args.verbose != 0)?;
        }
        return Ok(());
    }

    clean_ws(&ws, &ws.metadata.workspace_members, &args.clean, args.verbose)?;

    Ok(())
}

// TODO: remove need for this.
// If --no-clean, --no-run, or --no-report is used: do not remove artifacts
// Otherwise, remove the followings to avoid false positives/false negatives:
// - build artifacts of crates to be measured for coverage
// - profdata
// - profraw
// - doctest bins
// - old reports
pub(crate) fn clean_partial(cx: &Context) -> Result<()> {
    if cx.args.clean.no_clean {
        return Ok(());
    }

    clean_ws_inner(&cx.ws, &cx.workspace_members.included, cx.args.verbose > 1, false)?;

    let mut package_args =
        Vec::with_capacity((cx.workspace_members.included.len() + cx.args.dep_coverage.len()) * 2);
    for &id in &cx.workspace_members.included {
        package_args.push("--package");
        package_args.push(&cx.ws.metadata[id].name);
    }
    for dep in &cx.args.dep_coverage {
        package_args.push("--package");
        package_args.push(dep);
    }
    let mut cmd = cx.cargo();
    cmd.arg("clean").args(package_args);
    cargo::clean_args(cx, &mut cmd);
    if let Err(e) = if cx.args.verbose > 1 { cmd.run() } else { cmd.run_with_output() } {
        warn!("{e:#}");
    }

    Ok(())
}

fn clean_ws(
    ws: &Workspace,
    pkg_ids: &[PackageId],
    options: &CleanOptions,
    verbose: u8,
) -> Result<()> {
    clean_ws_inner(ws, pkg_ids, verbose != 0, options.profraw_only)?;

    if options.profraw_only {
        return Ok(());
    }

    let mut package_args = Vec::with_capacity(pkg_ids.len() * 2);
    for &id in pkg_ids {
        package_args.push("--package");
        package_args.push(&ws.metadata[id].name);
    }
    let mut args_set = vec![vec![]];
    if ws.target_dir.join("release").exists() {
        args_set.push(vec!["--release"]);
    }
    let target_list = ws.rustc_print("target-list")?;
    for target in target_list.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if ws.target_dir.join(target).exists() {
            args_set.push(vec!["--target", target]);
        }
    }
    for args in args_set {
        let mut cmd = ws.cargo(verbose);
        cmd.args(["clean", "--target-dir", ws.target_dir.as_str()]).args(&package_args);
        if let Some(build_dir) = &ws.build_dir {
            cmd.env("CARGO_BUILD_BUILD_DIR", build_dir.as_str());
        }
        cmd.args(args);
        if verbose > 0 {
            cmd.arg(format!("-{}", "v".repeat(verbose as usize)));
        }
        options.cargo_args(&mut cmd);
        cmd.dir(&ws.metadata.workspace_root);
        if let Err(e) = if verbose > 0 { cmd.run() } else { cmd.run_with_output() } {
            warn!("{e:#}");
        }
    }
    Ok(())
}

fn clean_ws_inner(
    ws: &Workspace,
    pkg_ids: &[PackageId],
    verbose: bool,
    profraw_only: bool,
) -> Result<()> {
    clean_profraw_files(ws, verbose)?;

    if profraw_only {
        return Ok(());
    }

    for format in &["html", "text"] {
        rm_rf(ws.default_output_dir.join(format), verbose)?;
    }

    rm_rf(&ws.doctests_dir, verbose)?;
    rm_rf(&ws.profdata_file, verbose)?;

    clean_trybuild_artifacts(ws, pkg_ids, verbose)?;
    clean_ui_test_artifacts(ws, verbose)?;
    Ok(())
}

fn clean_profraw_files(ws: &Workspace, verbose: bool) -> Result<()> {
    for path in glob::glob(
        Utf8Path::new(&glob::Pattern::escape(ws.target_dir.as_str())).join("*.profraw").as_str(),
    )?
    .filter_map(Result::ok)
    {
        rm_rf(path, verbose)?;
    }
    Ok(())
}

fn pkg_hash_re(ws: &Workspace, pkg_ids: &[PackageId]) -> RegexVec {
    let mut re = RegexVecBuilder::new("^(lib)?(", ")(-[0-9a-f]{7,})?$");
    for &id in pkg_ids {
        re.or(&ws.metadata[id].name.replace('-', "(-|_)"));
    }
    re.build().unwrap()
}

fn clean_trybuild_artifacts(ws: &Workspace, pkg_ids: &[PackageId], verbose: bool) -> Result<()> {
    let trybuild_target_dir = ws.trybuild_target_dir();
    let re = pkg_hash_re(ws, pkg_ids);

    for e in WalkDir::new(trybuild_target_dir).into_iter().filter_map(Result::ok) {
        let path = e.path();
        if let Some(file_stem) = fs::file_stem_recursive(path).unwrap().to_str() {
            if re.is_match(file_stem) {
                rm_rf(path, verbose)?;
            }
        }
    }
    Ok(())
}

fn clean_ui_test_artifacts(ws: &Workspace, verbose: bool) -> Result<()> {
    let ui_test_target_dir = ws.ui_test_target_dir();
    rm_rf(ui_test_target_dir, verbose)?;
    Ok(())
}

fn rm_rf(path: impl AsRef<Path>, verbose: bool) -> Result<()> {
    let path = path.as_ref();
    // Using std::fs instead of fs-err is okay here since we ignore error contents
    #[allow(clippy::disallowed_methods)]
    let m = std::fs::symlink_metadata(path);
    if m.as_ref().is_ok_and(fs::Metadata::is_dir) {
        if verbose {
            status!("Removing", "{}", path.display());
        }
        fs::remove_dir_all(path)?;
    } else if m.is_ok() {
        if verbose {
            status!("Removing", "{}", path.display());
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
