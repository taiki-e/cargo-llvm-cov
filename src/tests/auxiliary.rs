use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use once_cell::sync::Lazy;
use walkdir::WalkDir;

use crate::fs;

static FIXTURES_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"));

#[allow(single_use_lifetimes)]
#[track_caller]
pub(crate) fn cargo_llvm_cov<'a>(
    model: &str,
    name: &str,
    extension: &str,
    args: impl AsRef<[&'a str]>,
) -> Result<()> {
    let workspace_root = test_project(model, name)?;
    let manifest_path = workspace_root.join("Cargo.toml").display().to_string();
    let output_dir = FIXTURES_PATH.join("coverage-reports").join(model);
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(name).with_extension(extension);
    let mut v = vec![
        "cargo",
        "llvm-cov",
        "--color",
        "never",
        "--manifest-path",
        &manifest_path,
        "--output-path",
        output_path.as_str(),
    ];
    v.extend(args.as_ref().iter());
    crate::run(v)?;
    if env::var_os("CI").is_some() {
        process!("git", "--no-pager", "diff", "--exit-code", output_path).run()?;
    }
    Ok(())
}

fn test_project(model: &str, name: &str) -> Result<PathBuf> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("cargo-llvm-cov-tests")
        .join(&format!("{}-{}", model, name));
    let model_path = FIXTURES_PATH.join("coverage").join(model);

    fs::remove_dir_all(&workspace_root)?;
    fs::create_dir_all(&workspace_root)?;
    for entry in WalkDir::new(&model_path).into_iter().filter_map(Result::ok) {
        let from = entry.path();
        let to = &workspace_root.join(from.strip_prefix(&model_path)?);
        if from.is_dir() {
            fs::create_dir(to)?;
        } else {
            fs::copy(from, to)?;
        }
    }

    Ok(workspace_root)
}
