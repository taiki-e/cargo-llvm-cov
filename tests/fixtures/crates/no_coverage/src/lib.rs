#![cfg_attr(coverage, feature(no_coverage))]

fn func(x: i32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        3 => {}
        _ => {}
    }
}

#[cfg_attr(coverage, no_coverage)]
#[test]
fn fn_level() {
    func(0);

    if false {
        func(1);
    }
}

// #[no_coverage] has no effect on expressions.
#[test]
fn expr_level() {
    if false {
        #[cfg_attr(coverage, no_coverage)]
        func(2);
    }
}

// #[no_coverage] has no effect on modules.
#[cfg_attr(coverage, no_coverage)]
mod mod_level {
    use super::func;

    #[test]
    fn mod_level() {
        if false {
            func(3);
        }
    }
}
