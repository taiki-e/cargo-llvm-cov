#![warn(rust_2018_idioms)]

mod auxiliary;

use anyhow::Context as _;
use auxiliary::{
    assert_output, cargo_llvm_cov, normalize_output, perturb_one_header, test_project, test_report,
    CommandExt, FIXTURES_PATH,
};
use camino::Utf8Path;
use fs_err as fs;
use tempfile::tempdir;

fn test_set() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("txt", &["--text"]),
        ("hide-instantiations.txt", &["--text", "--hide-instantiations"]),
        ("summary.txt", &[]),
        ("json", &["--json", "--summary-only"]),
        // TODO: full JSON output is unstable between platform.
        // ("full.json", &["--json"]),
        ("lcov.info", &["--lcov", "--summary-only"]),
    ]
}

fn run(model: &str, name: &str, args: &[&str], envs: &[(&str, &str)]) {
    let id = format!("{model}/{name}");
    for (extension, args2) in test_set() {
        test_report(model, name, extension, None, &[args, args2].concat(), envs)
            .context(id.clone())
            .unwrap();
    }
}

// TODO:
// - add tests for non-crates.io dependencies

#[test]
fn real1() {
    run("real1", "workspace_root", &[], &[]);
    run("real1", "all", &["--all"], &[]);
    run("real1", "manifest_path", &["--manifest-path", "member1/member2/Cargo.toml"], &[]);
    run("real1", "package1", &["--package", "member2"], &[]);
    run("real1", "exclude", &["--all", "--exclude", "crate1"], &[]);
}

#[test]
fn virtual1() {
    run("virtual1", "workspace_root", &[], &[]);
    run("virtual1", "package1", &["--package", "member1"], &[]);
    run("virtual1", "package2", &["--package", "member1", "--package", "member2"], &[]);
    run("virtual1", "package2-multi-val", &["--package", "member1", "member2"], &[]);
    run("virtual1", "package3", &["--package", "member2"], &[]);
    run("virtual1", "package4", &["--package", "member3"], &[]);
    run("virtual1", "package5", &["--package", "member4"], &[]);
    run("virtual1", "package6", &["--package", "member3", "--package", "member4"], &[]);
    run("virtual1", "exclude", &["--workspace", "--exclude", "member2"], &[]);
    run(
        "virtual1",
        "exclude-from-report",
        &["--workspace", "--exclude-from-report", "member2"],
        &[],
    );
    run("virtual1", "exclude-from-test", &["--workspace", "--exclude-from-test", "member2"], &[]);
    run("virtual1", "exclude-multi-val", &["--workspace", "--exclude", "member1", "member2"], &[]);
}

#[test]
fn no_test() {
    // TODO: we should fix this: https://github.com/taiki-e/cargo-llvm-cov/issues/21
    run("no_test", "no_test", &[], &[]);
    if !(cfg!(windows) && cfg!(target_env = "msvc")) {
        run("no_test", "link_dead_code", &[], &[("RUSTFLAGS", "-C link-dead-code")]);
    }
}

#[test]
fn bin_crate() {
    run("bin_crate", "bin_crate", &[], &[]);

    let model = "bin_crate";
    let name = "run";
    let id = format!("{model}/{name}");
    for (extension, args2) in test_set() {
        test_report(model, name, extension, Some("run"), &[args2, &["--", "1"]].concat(), &[])
            .context(id.clone())
            .unwrap();
    }
}

#[test]
fn instantiations() {
    // TODO: fix https://github.com/taiki-e/cargo-llvm-cov/issues/43
    run("instantiations", "instantiations", &[], &[]);
}

#[test]
fn cargo_config() {
    run("cargo_config", "cargo_config", &[], &[]);
    run("cargo_config_toml", "cargo_config_toml", &[], &[]);
}

#[test]
fn no_coverage() {
    let model = "no_coverage";
    let id = format!("{model}/{model}");
    for (extension, args2) in test_set() {
        // TODO: On windows, the order of the instantiations in the generated coverage report will be different.
        if extension == "full.json" && cfg!(windows) {
            continue;
        }
        test_report(model, model, extension, None, args2, &[]).context(id.clone()).unwrap();
    }

    let name = "no_cfg_coverage";
    let id = format!("{model}/{name}");
    for (extension, args2) in test_set() {
        // TODO: On windows, the order of the instantiations in the generated coverage report will be different.
        if extension == "full.json" && cfg!(windows) {
            continue;
        }
        test_report(model, name, extension, None, &[args2, &["--no-cfg-coverage"]].concat(), &[])
            .context(id.clone())
            .unwrap();
    }
}

#[test]
fn merge() {
    let output_dir = FIXTURES_PATH.join("coverage-reports").join("merge");
    merge_with_failure_mode(&output_dir, false);
}

#[test]
fn merge_failure_mode_all() {
    let tempdir = tempdir().unwrap();
    let output_dir = Utf8Path::from_path(tempdir.path()).unwrap();
    merge_with_failure_mode(output_dir, true);
}

fn merge_with_failure_mode(output_dir: &Utf8Path, failure_mode_all: bool) {
    let model = "merge";
    fs::create_dir_all(&output_dir).unwrap();
    for (extension, args) in test_set() {
        let workspace_root = test_project(model).unwrap();
        let output_path = &output_dir.join(model).with_extension(extension);
        let expected = &fs::read_to_string(output_path).unwrap_or_default();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-report", "--features", "a"])
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-report", "--features", "b"])
            .current_dir(workspace_root.path())
            .assert_success();
        let mut cmd = cargo_llvm_cov();
        cmd.args(["--color", "never", "--no-run", "--output-path"])
            .arg(output_path)
            .args(args)
            .current_dir(workspace_root.path());
        cmd.assert_success();

        if failure_mode_all {
            perturb_one_header(workspace_root.path()).unwrap().unwrap();
            cmd.assert_failure()
                .stderr_contains("unrecognized instrumentation profile encoding format");
            cmd.args(&["--failure-mode", "all"]);
            cmd.assert_success();
        } else {
            normalize_output(output_path, args).unwrap();
            assert_output(output_path, expected).unwrap();
        }
    }
}

#[test]
fn clean_ws() {
    let model = "merge";
    let name = "clean_ws";
    let output_dir = FIXTURES_PATH.join("coverage-reports").join(model);
    fs::create_dir_all(&output_dir).unwrap();
    for (extension, args) in test_set() {
        let workspace_root = test_project(model).unwrap();
        let output_path = &output_dir.join(name).with_extension(extension);
        let expected = &fs::read_to_string(output_path).unwrap_or_default();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-report", "--features", "a"])
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-run", "--output-path"])
            .arg(output_path)
            .args(args)
            .current_dir(workspace_root.path())
            .assert_success();

        normalize_output(output_path, args).unwrap();
        assert_output(output_path, expected).unwrap();

        cargo_llvm_cov()
            .args(["clean", "--color", "never", "--workspace"])
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-report", "--features", "a"])
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov()
            .args(["--color", "never", "--no-run", "--output-path"])
            .arg(output_path)
            .args(args)
            .current_dir(workspace_root.path())
            .assert_success();

        normalize_output(output_path, args).unwrap();
        assert_output(output_path, expected).unwrap();
    }
}

#[cfg_attr(windows, ignore)] // `echo` may not be available
#[test]
fn open_report() {
    let model = "real1";
    let workspace_root = test_project(model).unwrap();
    cargo_llvm_cov()
        .args(["--color", "never", "--open"])
        .current_dir(workspace_root.path())
        .env("BROWSER", "echo")
        .assert_success()
        .stdout_contains(
            &workspace_root.path().join("target/llvm-cov/html/index.html").to_string_lossy(),
        );
}

#[test]
fn version() {
    cargo_llvm_cov().arg("--version").assert_success().stdout_contains(env!("CARGO_PKG_VERSION"));
    cargo_llvm_cov().args(["clean", "--version"]).assert_failure().stderr_contains(
        "Found argument '--version' which wasn't expected, or isn't valid in this context",
    );
}
