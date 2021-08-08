// https://github.com/taiki-e/cargo-llvm-cov/issues/43

fn func<T: Default + PartialOrd>(t: T) -> bool {
    if t < T::default() {
        true
    } else {
        false
    }
}

#[test]
fn test() {
    assert!(!func(1_f32));
    assert!(func(-1_i32));
}
