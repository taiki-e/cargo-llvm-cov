use std::process::Command;

#[test]
fn test() {
    // assert!(Command::new(env!("CARGO_BIN_EXE_continuous_mode"))
    //     .arg("0")
    //     .status()
    //     .unwrap()
    //     .success());

    let mut child = Command::new(env!("CARGO_BIN_EXE_continuous_mode")).arg("1").spawn().unwrap();
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs_f32(0.1));
    }
    child.kill().unwrap();
    std::thread::sleep(std::time::Duration::from_secs_f32(0.5));
}
