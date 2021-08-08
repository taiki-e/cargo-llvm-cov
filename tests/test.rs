mod auxiliary;

use anyhow::Context as _;
use auxiliary::cargo_llvm_cov;

fn run(model: &str, name: &str, args: &[&str]) {
    let id = format!("{}/{}", model, name);

    cargo_llvm_cov(model, name, "txt", {
        let mut v = vec!["--text"];
        v.extend_from_slice(args);
        v
    })
    .context(id.clone())
    .unwrap();

    cargo_llvm_cov(model, name, "summary.txt", args).context(id.clone()).unwrap();

    cargo_llvm_cov(model, name, "json", {
        let mut v = vec!["--json", "--summary-only"];
        v.extend_from_slice(args);
        v
    })
    .context(id.clone())
    .unwrap();

    cargo_llvm_cov(model, name, "full.json", {
        let mut v = vec!["--json"];
        v.extend_from_slice(args);
        v
    })
    .context(id.clone())
    .unwrap();

    cargo_llvm_cov(model, name, "lcov.info", {
        let mut v = vec!["--lcov", "--summary-only"];
        v.extend_from_slice(args);
        v
    })
    .context(id)
    .unwrap();
}

// TODO:
// - add tests for non-crates.io dependencies

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn real1() {
    run("real1", "workspace_root", &[]);
    run("real1", "workspace_root_all", &["--all"]);
    run("real1", "workspace_root_member2_manifest_path", &[
        "--manifest-path",
        "member1/member2/Cargo.toml",
    ]);
    run("real1", "workspace_root_member2_package", &["--package", "member2"]);
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn virtual1() {
    run("virtual1", "workspace_root", &[]);
    run("virtual1", "workspace_root_member1_package", &["--package", "member1"]);
    run("virtual1", "workspace_root_member1_2_package", &[
        "--package",
        "member1",
        "--package",
        "member2",
    ]);
    // TODO: member2/member3 and member2/src/member4 should not be excluded.
    run("virtual1", "exclude", &["--workspace", "--exclude", "member2"]);
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn no_test() {
    // TODO: we should fix this: https://github.com/taiki-e/cargo-llvm-cov/issues/21
    run("no_test", "no_test", &[]);
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn bin_crate() {
    run("bin_crate", "bin_crate", &[]);
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn instantiations() {
    // TODO: fix https://github.com/taiki-e/cargo-llvm-cov/issues/43
    run("instantiations", "instantiations", &[]);
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn cargo_config() {
    run("cargo_config", "cargo_config", &[]);
    run("cargo_config_toml", "cargo_config_toml", &[]);
}
