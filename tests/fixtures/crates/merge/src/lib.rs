fn func(x: i32) -> bool {
    if x < 0 { true } else { false }
}

#[test]
fn test() {
    #[cfg(feature = "a")]
    assert!(!func(1));
    #[cfg(feature = "b")]
    assert!(func(-1));
}
