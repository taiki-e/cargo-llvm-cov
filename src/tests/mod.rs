mod auxiliary;

use anyhow::Context as _;
use auxiliary::cargo_llvm_cov;

fn set(model: &str, name: &str) {
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
fn test() {
    set("real1", "workspace_root");
    set("virtual1", "workspace_root");
}

// TODO:
// - add tests for non-crates.io dependencies
// - add tests for --exclude
