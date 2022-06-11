pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn foo() -> BoxResult<()> {
    Ok(())
}

pub fn bar() -> BoxResult<()> {
    Ok(foo()?)
}

#[derive(serde::Deserialize)]
struct RelationDict {
}

pub fn baz() -> i32 { 42 }

pub fn blah() -> i32 { 42 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        bar().unwrap();

        let j = serde_json::json!({});
        let _: RelationDict = serde_json::from_value(j).unwrap();
    }
}
