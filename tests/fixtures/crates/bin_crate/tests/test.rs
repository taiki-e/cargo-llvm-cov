use std::process::Command;

#[test]
fn test() {
    assert!(Command::new(env!("CARGO_BIN_EXE_bin_crate")).arg("0").status().unwrap().success());

    // The RUSTFLAGS environment variable is applied at compile time,
    // so it does not need to be present at run time of the test.
    // (i.e., the profile of this test will be collected.)
    assert!(Command::new(env!("CARGO_BIN_EXE_bin_crate"))
        .arg("1")
        .env_remove("RUSTFLAGS")
        .status()
        .unwrap()
        .success());

    // If you remove the LLVM_PROFILE_FILE environment variable,
    // no profile will be collected.
    assert!(Command::new(env!("CARGO_BIN_EXE_bin_crate"))
        .arg("2")
        .env_remove("LLVM_PROFILE_FILE")
        .status()
        .unwrap()
        .success());
}
