pub fn func(x: u32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
}

pub fn func2(x: u32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
}

#[test]
fn test() {
    func2(0);
    func2(2);
}
