// SPDX-License-Identifier: Apache-2.0 OR MIT

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
