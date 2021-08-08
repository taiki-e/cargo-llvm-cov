fn func(x: i32) -> bool {
    if x < 0 {
        true
    } else {
        false
    }
}

#[test]
fn test() {
    #[cfg(a)]
    assert!(!func(1));
    #[cfg(not(a))]
    assert!(func(-1));
}
