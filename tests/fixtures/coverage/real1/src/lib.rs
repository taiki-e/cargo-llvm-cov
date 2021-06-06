pub fn match1(x: u32) {
    match x {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
}

#[test]
fn test() {
    match1(1);
    match1(3);
}
