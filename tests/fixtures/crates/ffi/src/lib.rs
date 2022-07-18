#![warn(rust_2018_idioms, unsafe_op_in_unsafe_fn)]

extern "C" {
    fn hello_c();
    fn hello_cpp();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        println!("Hello Rust!");
        unsafe {
            hello_c();
            hello_cpp();
        }
    }
}
