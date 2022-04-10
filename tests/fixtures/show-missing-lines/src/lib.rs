pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn foo() -> BoxResult<()> {
    Ok(())
}

pub fn bar() -> BoxResult<()> {
    Ok(foo()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        foo().unwrap();
    }
}
