#![warn(rust_2018_idioms)]

use anyhow::Context as _;
use auxiliary::{
    assert_output, cargo_llvm_cov, fixtures_path, normalize_output, perturb_one_header,
    test_project, test_report, CommandExt,
};
use camino::Utf8Path;
use fs_err as fs;
use tempfile::tempdir;

mod auxiliary;

const SUBCOMMANDS: &[&str] = &["", "run", "report", "clean", "show-env", "nextest"];

fn test_set() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("txt", &["--text"]),
        ("hide-instantiations.txt", &["--text", "--hide-instantiations"]),
        ("summary.txt", &[]),
        ("json", &["--json", "--summary-only"]),
        // TODO: full JSON output is unstable between platform.
        // ("full.json", &["--json"]),
        ("lcov.info", &["--lcov", "--summary-only"]),
        // TODO: test Cobertura output
        ("codecov.json", &["--codecov"]),
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
    run("virtual1", "package3", &["--package", "member2"], &[]);
    run("virtual1", "package4", &["--package", "member3"], &[]);
    run("virtual1", "package5", &["--package", "member4"], &[]);
    run("virtual1", "package6", &["--package", "member3", "--package", "member4"], &[]);
    run("virtual1", "exclude1", &["--workspace", "--exclude", "member1"], &[]);
    run("virtual1", "exclude2", &["--workspace", "--exclude", "member2"], &[]);
    run(
        "virtual1",
        "exclude-from-report1",
        &["--workspace", "--exclude-from-report", "member1"],
        &[],
    );
    run(
        "virtual1",
        "exclude-from-report2",
        &["--workspace", "--exclude-from-report", "member2"],
        &[],
    );
    run("virtual1", "exclude-from-test1", &["--workspace", "--exclude-from-test", "member1"], &[]);
    run("virtual1", "exclude-from-test2", &["--workspace", "--exclude-from-test", "member2"], &[]);
}

#[test]
fn no_test() {
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

#[rustversion::attr(not(nightly), ignore)]
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

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn coverage_helper() {
    let model = "coverage_helper";
    let id = format!("{model}/{model}");
    for (extension, args2) in test_set() {
        // TODO: On windows, the order of the instantiations in the generated coverage report will be different.
        if extension == "full.json" && cfg!(windows) {
            continue;
        }
        test_report(model, model, extension, None, args2, &[]).context(id.clone()).unwrap();
    }
}

// The order of the instantiations in the generated coverage report will be different depending on the version.
#[rustversion::attr(not(nightly), ignore)]
#[test]
fn merge() {
    let output_dir = fixtures_path().join("coverage-reports").join("merge");
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
    fs::create_dir_all(output_dir).unwrap();
    for (extension, args) in test_set() {
        let workspace_root = test_project(model).unwrap();
        let output_path = &output_dir.join(model).with_extension(extension);
        let expected = &fs::read_to_string(output_path).unwrap_or_default();
        cargo_llvm_cov("")
            .args(["--color", "never", "--no-report", "--features", "a"])
            .arg("--remap-path-prefix")
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov("")
            .args(["--color", "never", "--no-report", "--features", "b"])
            .arg("--remap-path-prefix")
            .current_dir(workspace_root.path())
            .assert_success();
        let mut cmd = cargo_llvm_cov("report");
        cmd.args(["--color", "never", "--output-path"])
            .arg(output_path)
            .arg("--remap-path-prefix")
            .args(args)
            .current_dir(workspace_root.path());
        cmd.assert_success();

        if failure_mode_all {
            perturb_one_header(workspace_root.path()).unwrap().unwrap();
            cmd.assert_failure()
                .stderr_contains("unrecognized instrumentation profile encoding format");
            cmd.args(["--failure-mode", "all"]);
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
    let output_dir = fixtures_path().join("coverage-reports").join(model);
    fs::create_dir_all(&output_dir).unwrap();
    for (extension, args) in test_set() {
        let workspace_root = test_project(model).unwrap();
        let output_path = &output_dir.join(name).with_extension(extension);
        let expected = &fs::read_to_string(output_path).unwrap_or_default();
        cargo_llvm_cov("")
            .args(["--color", "never", "--no-report", "--features", "a"])
            .arg("--remap-path-prefix")
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov("report")
            .args(["--color", "never", "--output-path"])
            .arg(output_path)
            .arg("--remap-path-prefix")
            .args(args)
            .current_dir(workspace_root.path())
            .assert_success();

        normalize_output(output_path, args).unwrap();
        assert_output(output_path, expected).unwrap();

        cargo_llvm_cov("")
            .args(["clean", "--color", "never", "--workspace"])
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov("")
            .args(["--color", "never", "--no-report", "--features", "a"])
            .arg("--remap-path-prefix")
            .current_dir(workspace_root.path())
            .assert_success();
        cargo_llvm_cov("report")
            .args(["--color", "never", "--output-path"])
            .arg(output_path)
            .arg("--remap-path-prefix")
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
    cargo_llvm_cov("")
        .args(["--color", "never", "--open"])
        .current_dir(workspace_root.path())
        .env("BROWSER", "echo")
        .assert_success()
        .stdout_contains(
            workspace_root.path().join("target/llvm-cov/html/index.html").to_string_lossy(),
        );
}

#[test]
fn show_env() {
    cargo_llvm_cov("show-env").assert_success().stdout_not_contains("export");
    cargo_llvm_cov("show-env").arg("--export-prefix").assert_success().stdout_contains("export");
}

#[allow(clippy::single_element_loop)]
#[test]
fn invalid_arg() {
    for subcommand in ["", "run", "clean", "show-env", "nextest"] {
        if subcommand != "show-env" {
            cargo_llvm_cov(subcommand)
                .arg("--export-prefix")
                .assert_failure()
                .stderr_contains("invalid option '--export-prefix'");
        }
        if !subcommand.is_empty() {
            if subcommand == "nextest" {
                cargo_llvm_cov(subcommand)
                    .arg("--doc")
                    .assert_failure()
                    .stderr_contains("doctest is not supported for nextest");
                cargo_llvm_cov(subcommand)
                    .arg("--doctests")
                    .assert_failure()
                    .stderr_contains("doctest is not supported for nextest");
            } else {
                cargo_llvm_cov(subcommand)
                    .arg("--doc")
                    .assert_failure()
                    .stderr_contains("invalid option '--doc'");
                cargo_llvm_cov(subcommand)
                    .arg("--doctests")
                    .assert_failure()
                    .stderr_contains("invalid option '--doctests'");
            }
        }
        if !matches!(subcommand, "" | "nextest") {
            for arg in [
                "--lib",
                "--bins",
                "--examples",
                "--test=v",
                "--tests",
                "--bench=v",
                "--benches",
                "--all-targets",
                "--no-run",
                "--no-fail-fast",
                "--exclude=v",
                "--exclude-from-test=v",
            ] {
                cargo_llvm_cov(subcommand).arg(arg).assert_failure().stderr_contains(format!(
                    "invalid option '{}' for subcommand '{subcommand}'",
                    arg.strip_suffix("=v").unwrap_or(arg)
                ));
            }
        }
        if !matches!(subcommand, "" | "nextest" | "run") {
            for arg in [
                "--bin=v",
                "--example=v",
                "--exclude-from-report=v",
                "--no-cfg-coverage",
                "--no-cfg-coverage-nightly",
                "--no-report",
                "--no-clean",
                "--ignore-run-fail",
            ] {
                cargo_llvm_cov(subcommand).arg(arg).assert_failure().stderr_contains(format!(
                    "invalid option '{}' for subcommand '{subcommand}'",
                    arg.strip_suffix("=v").unwrap_or(arg)
                ));
            }
        }
        if !matches!(subcommand, "" | "nextest" | "clean") {
            for arg in ["--workspace"] {
                cargo_llvm_cov(subcommand).arg(arg).assert_failure().stderr_contains(format!(
                    "invalid option '{}' for subcommand '{subcommand}'",
                    arg.strip_suffix("=v").unwrap_or(arg)
                ));
            }
        }
    }
}

#[test]
fn invalid_arg_no_passthrough() {
    // These subcommands don't allow passthrough args.
    // In other subcommands, if passthrough args are invalid,
    // it will be detected by cargo or cargo-nextest.
    for subcommand in ["report", "clean", "show-env"] {
        cargo_llvm_cov(subcommand)
            .arg("-a")
            .assert_failure()
            .stderr_contains(format!("invalid option '-a' for subcommand '{subcommand}'"));
        cargo_llvm_cov(subcommand)
            .arg("--b")
            .assert_failure()
            .stderr_contains(format!("invalid option '--b' for subcommand '{subcommand}'"));
        cargo_llvm_cov(subcommand)
            .arg("c")
            .assert_failure()
            .stderr_contains("unexpected argument \"c\"");
    }
}

#[test]
fn help() {
    for &subcommand in SUBCOMMANDS {
        cargo_llvm_cov(subcommand)
            .arg("--help")
            .assert_success()
            .stdout_contains(format!("cargo llvm-cov {subcommand}"));
    }
}

#[test]
fn version() {
    for &subcommand in SUBCOMMANDS {
        if subcommand.is_empty() {
            cargo_llvm_cov(subcommand)
                .arg("--version")
                .assert_success()
                .stdout_contains(env!("CARGO_PKG_VERSION"));
        } else {
            cargo_llvm_cov(subcommand).arg("--version").assert_failure().stderr_contains(format!(
                "invalid option '--version' for subcommand '{subcommand}'"
            ));
        }
    }
}
