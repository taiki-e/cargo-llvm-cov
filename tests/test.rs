mod auxiliary;

use anyhow::Context as _;
use auxiliary::cargo_llvm_cov;

fn run(model: &str, name: &str, args: &[&str]) {
    let id = format!("{}/{}", model, name);
    cargo_llvm_cov(model, name, "summary.txt", args).context(id.clone()).unwrap();

    cargo_llvm_cov(model, name, "json", {
        let mut v = vec!["--json", "--summary-only"];
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
    .context(id.clone())
    .unwrap();

    cargo_llvm_cov(model, name, "txt", {
        let mut v = vec!["--text"];
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
fn real_root() {
    run("real1", "workspace_root", &[]);
    run("real1", "workspace_root_all", &["--all"]);
    run("real1", "workspace_root_member2", &["--manifest-path", "member1/member2/Cargo.toml"]);

    run("virtual1", "workspace_root", &[]);

    // TODO: member2/member3 and member2/src/member4 should not be excluded.
    run("virtual1", "exclude", &["--workspace", "--exclude", "member2"]);

    // TODO: we should fix this: https://github.com/taiki-e/cargo-llvm-cov/issues/21
    run("no_test", "no_test", &[]);

    run("bin_crate", "bin_crate", &[]);
}
