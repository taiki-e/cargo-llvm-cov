// Refs:
// - https://doc.rust-lang.org/nightly/cargo/commands/cargo-clean.html
// - https://github.com/rust-lang/cargo/blob/0.62.0/src/cargo/ops/cargo_clean.rs

use std::{collections::BTreeSet, path::Path};

use anyhow::Result;
use cargo_metadata::PackageId;
use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{cargo::Workspace, cli::Args, context::Context, fs, term};

pub(crate) fn run(options: &mut Args) -> Result<()> {
    let ws = Workspace::new(&options.manifest, None, false, false)?;
    ws.config.merge_to_args(&mut None, &mut options.build.verbose, &mut options.build.color);
    term::set_coloring(&mut options.build.color);

    if !options.workspace {
        for dir in &[&ws.target_dir, &ws.output_dir] {
            rm_rf(dir, options.build.verbose != 0)?;
        }
        return Ok(());
    }

    clean_ws(&ws, &ws.metadata.workspace_members, options.build.verbose != 0)?;

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
    if cx.build.no_clean {
        return Ok(());
    }

    clean_ws(&cx.ws, &cx.workspace_members.included, cx.build.verbose > 1)?;

    Ok(())
}

fn clean_ws(ws: &Workspace, pkg_ids: &[PackageId], verbose: bool) -> Result<()> {
    let mut current_info = CargoLlvmCovInfo::current(ws);
    let info_file = ws.target_dir.join(".cargo_llvm_cov_info.json");
    let mut prev_info = None;
    if info_file.is_file() {
        if let Ok(info) = serde_json::from_str::<CargoLlvmCovInfo>(&fs::read_to_string(&info_file)?)
        {
            prev_info = Some(info);
        }
    }
    fs::create_dir_all(&ws.target_dir)?;
    fs::write(info_file, serde_json::to_vec(&current_info).unwrap())?;
    match prev_info {
        Some(prev_info) => {
            current_info.packages.extend(prev_info.packages);
            current_info.targets.extend(prev_info.targets);
        }
        None => {
            // TODO: warn if there are old artifacts and the info file is not valid
        }
    }

    for format in &["html", "text"] {
        rm_rf(ws.output_dir.join(format), verbose)?;
    }

    for entry in fs::read_dir(&ws.target_dir)?.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |e| e == "profraw") {
            rm_rf(path, verbose)?;
        }
    }

    rm_rf(&ws.doctests_dir, verbose)?;
    rm_rf(&ws.profdata_file, verbose)?;

    let re = &current_info.pkg_hash_re();
    clean_matched(&ws.target_dir, re, verbose)?;

    clean_trybuild_artifacts(ws, pkg_ids, verbose)?;
    Ok(())
}

fn clean_trybuild_artifacts(ws: &Workspace, pkg_ids: &[PackageId], verbose: bool) -> Result<()> {
    let trybuild_dir = &ws.metadata.target_directory.join("tests");
    let trybuild_target = &trybuild_dir.join("target");

    let mut re = String::from("^(lib)?(");
    let mut first = true;
    for id in pkg_ids {
        if first {
            first = false;
        } else {
            re.push('|');
        }
        re.push_str(&ws.metadata[id].name.replace('-', "(-|_)"));
    }
    re.push_str(")(-[0-9a-f]+)?$");
    // unwrap -- it is not realistic to have a case where there are more than
    // 5000 members in a workspace. see also pkg_hash_re_size_limit test.
    let re = &Regex::new(&re).unwrap();

    clean_matched(trybuild_target, re, verbose)
}

fn clean_matched(dir: impl AsRef<Path>, re: &Regex, verbose: bool) -> Result<()> {
    for e in WalkDir::new(dir.as_ref()).into_iter().filter_map(Result::ok) {
        let path = e.path();
        if let Some(file_stem) = fs::file_stem_recursive(path).unwrap().to_str() {
            if file_stem != "build-script-build" && re.is_match(file_stem) {
                rm_rf(path, verbose)?;
            }
        }
    }
    Ok(())
}

fn rm_rf(path: impl AsRef<Path>, verbose: bool) -> Result<()> {
    let path = path.as_ref();
    let m = fs::symlink_metadata(path);
    if m.as_ref().map(fs::Metadata::is_dir).unwrap_or(false) {
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoLlvmCovInfo {
    packages: BTreeSet<String>,
    targets: BTreeSet<String>,
}

impl CargoLlvmCovInfo {
    fn current(ws: &Workspace) -> Self {
        let mut packages = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for id in &ws.metadata.workspace_members {
            let pkg = &ws.metadata[id];
            packages.insert(pkg.name.clone());
            for t in &pkg.targets {
                targets.insert(t.name.clone());
            }
        }
        Self { packages, targets }
    }

    fn pkg_hash_re(&self) -> Regex {
        let mut re = String::from("^(lib)?(");
        let mut first = true;
        for pkg in &self.packages {
            if first {
                first = false;
            } else {
                re.push('|');
            }
            re.push_str(&pkg.replace('-', "(-|_)"));
        }
        for t in &self.targets {
            re.push('|');
            re.push_str(t);
        }
        re.push_str(")(-[0-9a-f]+)?$");
        // unwrap -- it is not realistic to have a case where there are more than
        // 5000 members in a workspace. see also pkg_hash_re_size_limit test.
        Regex::new(&re).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    fn pkg_hash_re(pkg_names: &[String]) -> Result<Regex, regex::Error> {
        let mut re = String::from("^(lib)?(");
        let mut first = true;
        for name in pkg_names {
            if first {
                first = false;
            } else {
                re.push('|');
            }
            re.push_str(&name.replace('-', "(-|_)"));
        }
        re.push_str(")(-[0-9a-f]+)?$");
        Regex::new(&re)
    }

    #[test]
    fn pkg_hash_re_size_limit() {
        fn gen_pkg_names(num_pkg: usize, pkg_name_size: usize) -> Vec<String> {
            (0..num_pkg)
                .map(|_| ('a'..='z').cycle().take(pkg_name_size).collect())
                .collect::<Vec<_>>()
        }

        let names = gen_pkg_names(5040, 64);
        pkg_hash_re(&names).unwrap();
        let names = gen_pkg_names(5041, 64);
        pkg_hash_re(&names).unwrap_err();

        let names = gen_pkg_names(2540, 128);
        pkg_hash_re(&names).unwrap();
        let names = gen_pkg_names(2541, 128);
        pkg_hash_re(&names).unwrap_err();

        let names = gen_pkg_names(1274, 256);
        pkg_hash_re(&names).unwrap();
        let names = gen_pkg_names(1275, 256);
        pkg_hash_re(&names).unwrap_err();
    }
}
