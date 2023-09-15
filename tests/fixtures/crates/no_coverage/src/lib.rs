#![cfg_attr(coverage, feature(coverage_attribute))]

fn func(x: i32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        3 => {}
        _ => {}
    }
}

#[cfg_attr(coverage, coverage(off))]
#[test]
fn fn_level() {
    func(0);

    if false {
        func(1);
    }
}

// #[coverage(off)] has no effect on expressions.
#[test]
fn expr_level() {
    if false {
        #[cfg_attr(coverage, coverage(off))]
        func(2);
    }
}

// #[coverage(off)] has no effect on modules.
#[cfg_attr(coverage, coverage(off))]
mod mod_level {
    use super::func;

    #[test]
    fn mod_level() {
        if false {
            func(3);
        }
    }
}
