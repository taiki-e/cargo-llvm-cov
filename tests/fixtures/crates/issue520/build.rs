// Simulates an extracted build-script $OUT_DIR. The executable name matches
// a package hash, the worst case for the object-file filters.
fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    let src = std::env::var("CARGO").unwrap();
    let dst = std::path::Path::new(&out).join("issue520-deadbeef");
    std::fs::copy(&src, &dst).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&dst).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dst, perms).unwrap();
    }
    std::fs::write(std::path::Path::new(&out).join("foo.o"), b"not-an-object").unwrap();
}
