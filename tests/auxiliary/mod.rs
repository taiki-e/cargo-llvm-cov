// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    env,
    ffi::OsStr,
    io::{Read, Seek, Write},
    mem,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    str,
    sync::Once,
};

use anyhow::Context as _;
use easy_ext::ext;
use fs_err as fs;

pub(crate) fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn ensure_llvm_tools_installed() {
    static TEST_VERSION: Once = Once::new();
    TEST_VERSION.call_once(|| {
        // Install component first to avoid component installation conflicts.
        let _ = Command::new("rustup").args(["component", "add", "llvm-tools-preview"]).output();
    });
}

pub(crate) fn cargo_llvm_cov(subcommand: &str) -> Command {
    ensure_llvm_tools_installed();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-llvm-cov"));
    cmd.arg("llvm-cov");
    if !subcommand.is_empty() {
        cmd.arg(subcommand);
    }
    cmd.env("CARGO_LLVM_COV_DENY_WARNINGS", "1");
    cmd.env_remove("RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS")
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
pub(crate) fn test_report(
    model: &str,
    name: &str,
    extension: &str,
    subcommand: Option<&str>,
    args: &[&str],
    envs: &[(&str, &str)],
) {
    eprintln!("model={model}/name={name}");
    let workspace_root = test_project(model);
    let output_dir = fixtures_path().join("coverage-reports").join(model);
    fs::create_dir_all(&output_dir).unwrap();
    let output_path = &output_dir.join(name).with_extension(extension);
    let expected = &fs::read_to_string(output_path).unwrap_or_default();
    let mut cmd = cargo_llvm_cov("");
    if let Some(subcommand) = subcommand {
        cmd.arg(subcommand);
    }
    cmd.args(["--color", "never", "--output-path"])
        .arg(output_path)
        .arg("--remap-path-prefix")
        .args(args)
        .current_dir(workspace_root.path());
    for (key, val) in envs {
        cmd.env(key, val);
    }
    cmd.assert_success();

    normalize_output(output_path, args);
    assert_output(output_path, expected);
}

#[track_caller]
pub(crate) fn assert_output(output_path: &Path, expected: &str) {
    if env::var_os("CI").is_some() {
        let mut child = Command::new("git")
            .args(["--no-pager", "diff", "--no-index", "--"])
            .arg("-")
            .arg(output_path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(expected.as_bytes()).unwrap();
        assert!(child.wait().unwrap().success());
    }
}

#[track_caller]
pub(crate) fn normalize_output(output_path: &Path, args: &[&str]) {
    if args.contains(&"--json") {
        let s = fs::read_to_string(output_path).unwrap();
        let mut json = serde_json::from_str::<cargo_llvm_cov::json::LlvmCovJsonExport>(&s).unwrap();
        if !args.contains(&"--summary-only") {
            json.demangle();
        }
        fs::write(output_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    }
    if cfg!(windows) {
        let s = fs::read_to_string(output_path).unwrap();
        // In json \ is escaped ("\\\\"), in other it is not escaped ("\\").
        fs::write(output_path, s.replace("\\\\", "/").replace('\\', "/")).unwrap();
    }
}

#[track_caller]
pub(crate) fn test_project(model: &str) -> tempfile::TempDir {
    let tmpdir = tempfile::tempdir().unwrap();
    let workspace_root = tmpdir.path();
    let model_path = fixtures_path().join("crates").join(model);

    for (file_name, from) in git_ls_files(&model_path, &[]) {
        let to = &workspace_root.join(file_name);
        if !to.parent().unwrap().is_dir() {
            fs::create_dir_all(to.parent().unwrap()).unwrap();
        }
        fs::copy(from, to).unwrap();
    }

    tmpdir
}

#[track_caller]
fn git_ls_files(dir: &Path, filters: &[&str]) -> Vec<(String, PathBuf)> {
    let mut cmd = Command::new("git");
    cmd.arg("ls-files").args(filters).current_dir(dir);
    let output =
        cmd.output().unwrap_or_else(|e| panic!("could not execute process `{cmd:?}`: {e}"));
    assert!(
        output.status.success(),
        "process didn't exit successfully: `{cmd:?}`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
        "-".repeat(60),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter_map(|f| {
            if f.is_empty() {
                return None;
            }
            let p = dir.join(f);
            if !p.exists() {
                return None;
            }
            Some((f.to_owned(), p))
        })
        .collect()
}

pub(crate) fn perturb_one_header(workspace_root: &Path) -> Option<PathBuf> {
    let target_dir = workspace_root.join("target").join("llvm-cov-target");
    let path = fs::read_dir(target_dir).unwrap().find_map(|entry| {
        let path = entry.ok().unwrap().path();
        if path.extension() == Some(OsStr::new("profraw")) {
            Some(path)
        } else {
            None
        }
    });
    if let Some(path) = &path {
        perturb_header(path);
    }
    path
}

const INSTR_PROF_RAW_MAGIC_64: u64 = (255_u64 << 56)
    | (('l' as u64) << 48)
    | (('p' as u64) << 40)
    | (('r' as u64) << 32)
    | (('o' as u64) << 24)
    | (('f' as u64) << 16)
    | (('r' as u64) << 8)
    | 129_u64;

fn perturb_header(path: &Path) {
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
    let mut magic = {
        let mut buf = vec![0_u8; mem::size_of::<u64>()];
        file.read_exact(&mut buf).unwrap();
        u64::from_ne_bytes(buf.try_into().unwrap())
    };
    assert_eq!(magic, INSTR_PROF_RAW_MAGIC_64);
    magic += 1;
    file.rewind().unwrap();
    file.write_all(&magic.to_ne_bytes()).unwrap();
}

#[ext(CommandExt)]
impl Command {
    #[track_caller]
    pub(crate) fn assert_output(&mut self) -> AssertOutput {
        let output = self.output().context("could not execute process").unwrap();
        AssertOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            status: output.status,
        }
    }

    #[track_caller]
    pub(crate) fn assert_success(&mut self) -> AssertOutput {
        let output = self.assert_output();
        assert!(
            output.status.success(),
            "assertion failed: `self.status.success()`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
            "-".repeat(60),
            output.stdout,
            output.stderr,
        );
        output
    }

    #[track_caller]
    pub(crate) fn assert_failure(&mut self) -> AssertOutput {
        let output = self.assert_output();
        assert!(
            !output.status.success(),
            "assertion failed: `!self.status.success()`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
            "-".repeat(60),
            output.stdout,
            output.stderr,
        );
        output
    }
}

pub(crate) struct AssertOutput {
    stdout: String,
    stderr: String,
    status: ExitStatus,
}

fn line_separated(lines: &str) -> impl Iterator<Item = &'_ str> {
    lines.lines().map(str::trim).filter(|line| !line.is_empty())
}

impl AssertOutput {
    /// Receives a line(`\n`)-separated list of patterns and asserts whether stderr contains each pattern.
    #[track_caller]
    pub(crate) fn stderr_contains(&self, pats: impl AsRef<str>) -> &Self {
        for pat in line_separated(pats.as_ref()) {
            assert!(
                self.stderr.contains(pat),
                "assertion failed: `self.stderr.contains(..)`:\n\nEXPECTED:\n{0}\n{pat}\n{0}\n\nACTUAL:\n{0}\n{1}\n{0}\n",
                "-".repeat(60),
                self.stderr
            );
        }
        self
    }

    /// Receives a line(`\n`)-separated list of patterns and asserts whether stdout contains each pattern.
    #[track_caller]
    pub(crate) fn stdout_contains(&self, pats: impl AsRef<str>) -> &Self {
        for pat in line_separated(pats.as_ref()) {
            assert!(
                self.stdout.contains(pat),
                "assertion failed: `self.stdout.contains(..)`:\n\nEXPECTED:\n{0}\n{pat}\n{0}\n\nACTUAL:\n{0}\n{1}\n{0}\n",
                "-".repeat(60),
                self.stdout
            );
        }
        self
    }

    /// Receives a line(`\n`)-separated list of patterns and asserts whether stdout contains each pattern.
    #[track_caller]
    pub(crate) fn stdout_not_contains(&self, pats: impl AsRef<str>) -> &Self {
        for pat in line_separated(pats.as_ref()) {
            assert!(
                !self.stdout.contains(pat),
                "assertion failed: `!self.stdout.contains(..)`:\n\nEXPECTED:\n{0}\n{pat}\n{0}\n\nACTUAL:\n{0}\n{1}\n{0}\n",
                "-".repeat(60),
                self.stdout
            );
        }
        self
    }
}
