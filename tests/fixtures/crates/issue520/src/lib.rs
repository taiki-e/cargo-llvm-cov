pub fn only_tested() -> u32 {
    7
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(super::only_tested(), 7);
    }
}
