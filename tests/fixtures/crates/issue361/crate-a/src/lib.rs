pub fn say_hello() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    #[test]
    fn test_say_hello() {
        super::say_hello();
    }
}
