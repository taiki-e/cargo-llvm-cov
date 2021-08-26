mod json;

use std::{
    env,
    process::{Command, ExitStatus},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use easy_ext::ext;
use fs_err as fs;
use once_cell::sync::Lazy;
use tempfile::{Builder, TempDir};
use walkdir::WalkDir;

pub static FIXTURES_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"));

pub fn cargo_llvm_cov() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-llvm-cov"));
    cmd.arg("llvm-cov");
    cmd.env_remove("RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .env_remove("CARGO_BUILD_RUSTDOCFLAGS")
        .env_remove("CARGO_TERM_VERBOSE")
        .env_remove("CARGO_TERM_COLOR")
        .env_remove("BROWSER")
        .env_remove("RUST_LOG")
        .env_remove("CI");
    cmd
}

#[track_caller]
pub fn test_report<'a>(
    model: &str,
    name: &str,
    extension: &str,
    args: impl AsRef<[&'a str]>,
) -> Result<()> {
    let args = args.as_ref();
    let workspace_root = test_project(model, name)?;
    let output_dir = FIXTURES_PATH.join("coverage-reports").join(model);
    fs::create_dir_all(&output_dir)?;
    let output_path = &output_dir.join(name).with_extension(extension);
    cargo_llvm_cov()
        .args(["--color", "never", "--output-path"])
        .arg(output_path)
        .args(args)
        .current_dir(workspace_root.path())
        .assert_success();

    normalize_output(output_path, args)?;
    assert_output(output_path)
}

pub fn assert_output(output_path: &Utf8Path) -> Result<()> {
    if env::var_os("CI").is_some() {
        assert!(Command::new("git")
            .args(&["--no-pager", "diff", "--exit-code"])
            .arg(output_path)
            .status()?
            .success());
    }
    Ok(())
}

pub fn normalize_output(output_path: &Utf8Path, args: &[&str]) -> Result<()> {
    if args.contains(&"--json") {
        let s = fs::read_to_string(output_path)?;
        let mut json = serde_json::from_str::<json::LlvmCovJsonExport>(&s).unwrap();
        if !args.contains(&"--summary-only") {
            json.demangle();
        }
        fs::write(output_path, serde_json::to_vec_pretty(&json)?)?;
    }
    #[cfg(windows)]
    {
        let s = fs::read_to_string(output_path)?;
        // In json \ is escaped ("\\\\"), in other it is not escaped ("\\").
        fs::write(output_path, s.replace("\\\\", "/").replace('\\', "/"))?;
    }
    Ok(())
}

pub fn test_project(model: &str, name: &str) -> Result<TempDir> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let tmpdir = Builder::new()
        .prefix(&format!("test_project_{}_{}_{}", model, name, COUNTER.fetch_add(1, Relaxed)))
        .tempdir()?;
    let workspace_root = tmpdir.path();
    let model_path = FIXTURES_PATH.join("crates").join(model);

    for entry in WalkDir::new(&model_path).into_iter().filter_map(Result::ok) {
        let from = entry.path();
        let to = &workspace_root.join(from.strip_prefix(&model_path)?);
        if from.is_dir() {
            fs::create_dir_all(to)?;
        } else {
            fs::copy(from, to)?;
        }
    }

    Ok(tmpdir)
}

#[ext(CommandExt)]
impl Command {
    #[track_caller]
    pub fn assert_output(&mut self) -> AssertOutput {
        let output = self.output().unwrap_or_else(|e| panic!("could not execute process: {}", e));
        AssertOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            status: output.status,
        }
    }

    #[track_caller]
    pub fn assert_success(&mut self) -> AssertOutput {
        let output = self.assert_output();
        if !output.status.success() {
            panic!(
                "assertion failed: `self.status.success()`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
                "-".repeat(60),
                output.stdout,
                output.stderr,
            );
        }
        output
    }

    #[track_caller]
    pub fn assert_failure(&mut self) -> AssertOutput {
        let output = self.assert_output();
        if output.status.success() {
            panic!(
                "assertion failed: `!self.status.success()`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
                "-".repeat(60),
                output.stdout,
                output.stderr,
            );
        }
        output
    }
}

pub struct AssertOutput {
    stdout: String,
    stderr: String,
    status: ExitStatus,
}

fn line_separated(lines: &str, f: impl FnMut(&str)) {
    lines.split('\n').map(str::trim).filter(|line| !line.is_empty()).for_each(f);
}

impl AssertOutput {
    // /// Receives a line(`\n`)-separated list of patterns and asserts whether stderr contains each pattern.
    // #[track_caller]
    // pub fn stderr_contains(&self, pats: &str) -> &Self {
    //     line_separated(pats, |pat| {
    //         if !self.stderr.contains(pat) {
    //             panic!(
    //                 "assertion failed: `self.stderr.contains(..)`:\n\nEXPECTED:\n{0}\n{1}\n{0}\n\nACTUAL:\n{0}\n{2}\n{0}\n",
    //                 "-".repeat(60),
    //                 pat,
    //                 self.stderr
    //             );
    //         }
    //     });
    //     self
    // }

    /// Receives a line(`\n`)-separated list of patterns and asserts whether stdout contains each pattern.
    #[track_caller]
    pub fn stdout_contains(&self, pats: &str) -> &Self {
        line_separated(pats, |pat| {
            if !self.stdout.contains(pat) {
                panic!(
                    "assertion failed: `self.stdout.contains(..)`:\n\nEXPECTED:\n{0}\n{1}\n{0}\n\nACTUAL:\n{0}\n{2}\n{0}\n",
                    "-".repeat(60),
                    pat,
                    self.stdout
                );
            }
        });
        self
    }
}
