fn main() {
    match std::env::args().skip(1).next().unwrap().parse::<u8>().unwrap() {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
}
