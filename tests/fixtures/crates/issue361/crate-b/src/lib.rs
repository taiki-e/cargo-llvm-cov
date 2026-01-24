pub fn say_hello_goodby() {
    crate_a::say_hello();
    println!("Goodby!");
}

#[cfg(test)]
mod test {
    #[test]
    fn test_say_hello_goodby() {
        super::say_hello_goodby();
    }
}
