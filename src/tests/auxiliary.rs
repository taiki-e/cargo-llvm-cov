use std::{
    env,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use duct::cmd;
use once_cell::sync::Lazy;
use structopt::StructOpt;
use tempfile::{Builder, TempDir};
use walkdir::WalkDir;

use crate::{fs, Opts};

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
    let test_project = test_project(&Utf8Path::new("coverage").join(model))?;
    let manifest_path = test_project.path().join("Cargo.toml").display().to_string();
    let output_path = FIXTURES_PATH.join("coverage-reports").join(model);
    fs::create_dir_all(&output_path)?;
    let output_path = output_path.join(name).with_extension(extension);
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
    let opts = Opts::from_iter_safe(v)?;
    crate::run(opts)?;
    if env::var_os("CI").is_some() {
        cmd!("git", "--no-pager", "diff", "--exit-code", output_path).run()?;
    }
    Ok(())
}

fn test_project(model_path: &Utf8Path) -> Result<TempDir> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let tmpdir = Builder::new()
        .prefix(&format!("test_project{}", COUNTER.fetch_add(1, Relaxed)))
        .tempdir()?;
    let workspace_root = tmpdir.path();
    let model_path = FIXTURES_PATH.join(model_path);

    for entry in WalkDir::new(&model_path).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        let tmppath = &workspace_root.join(path.strip_prefix(&model_path)?);
        if path.is_dir() {
            fs::create_dir(tmppath)?;
        } else {
            fs::copy(path, tmppath)?;
        }
    }

    Ok(tmpdir)
}
