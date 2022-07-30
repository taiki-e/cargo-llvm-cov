/*
This test does't currently tested on CI, and is intended to run locally.

$ cargo run
$ cargo llvm-cov run
cfg(a)
cfg(b)

$ CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS='--cfg c' cargo run
$ CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS='--cfg c' cargo llvm-cov run
cfg(a)
cfg(b)
cfg(c)

$ RUSTFLAGS='--cfg e' cargo run
$ RUSTFLAGS='--cfg e' cargo llvm-cov run
cfg(e)
*/

fn main() {
    #[cfg(a)]
    println!("cfg(a)");
    #[cfg(b)]
    println!("cfg(b)");
    #[cfg(c)]
    println!("cfg(c)");
    #[cfg(d)]
    println!("cfg(d)");
    #[cfg(e)]
    println!("cfg(e)");
}
