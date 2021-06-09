mod auxiliary;

use anyhow::Context as _;
use auxiliary::cargo_llvm_cov;

fn run(model: &str, name: &str) {
    let id = format!("{}/{}", model, name);
    cargo_llvm_cov(model, name, "summary.txt", []).context(id.clone()).unwrap();
    cargo_llvm_cov(model, name, "json", ["--json", "--summary-only"]).context(id.clone()).unwrap();
    cargo_llvm_cov(model, name, "lcov.info", ["--lcov", "--summary-only"])
        .context(id.clone())
        .unwrap();
    cargo_llvm_cov(model, name, "txt", ["--text"]).context(id).unwrap();
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn real1_workspace_root() {
    run("real1", "workspace_root");
}

// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn virtual1_workspace_root() {
    run("virtual1", "workspace_root");
}

// TODO: we should fix this: https://github.com/taiki-e/cargo-llvm-cov/issues/21
// It seems rustup is not installed in the docker image provided by cross.
#[cfg_attr(target_env = "musl", ignore)]
#[test]
fn no_test() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .init();
    run("no_test", "no_test");
}

// TODO:
// - add tests for non-crates.io dependencies
// - add tests for --exclude
