#![cfg_attr(coverage_nightly, feature(no_coverage))]

use coverage_helper::test;

fn func(x: i32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        3 => {}
        _ => {}
    }
}

#[test]
fn test() {
    func(0);

    if false {
        func(1);
    } else {
        func(2);
    }
}
