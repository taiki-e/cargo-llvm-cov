pub fn func(x: u32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
}

#[test]
fn test() {
    func(1);
    func(3);
}
