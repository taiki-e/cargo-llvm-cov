fn main() {
    match std::env::args().skip(1).next().unwrap().parse::<u8>().unwrap() {
        0 => {}
        1 => {}
        2 => {}
        _ => {}
    }
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs_f32(0.1));
    }
}
