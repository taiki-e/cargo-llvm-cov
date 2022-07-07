// src/tests is a well-known pattern: https://grep.app/search?q=%23%5Btest%5D&filter[path][0]=src/tests/&filter[lang][0]=Rust
//
// Note that to check that this pattern is properly supported,
// you need to run cargo-llvm-cov without --remap-path-prefix flag.
// (Our test suite always enables that flag.)

use super::*;

#[test]
fn test() {
    func(1);
    func(3);
    member1::func(0);
    member2::func(0);
}
