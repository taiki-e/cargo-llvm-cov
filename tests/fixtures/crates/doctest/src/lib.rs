/// ```
/// assert_eq!(crate1::generic_fn("doc", "doctest"), Ok("doctest"));
/// ```
pub fn generic_fn<T>(s: &str, val: T) -> Result<&str, T> {
    match s {
        "unit" => Ok("unit-test"),
        "doc" => Ok("doctest"),
        _ => Err(val),
    }
}

/// ```
/// assert_eq!(crate1::non_generic_fn("doc"), "doctest");
/// ```
pub fn non_generic_fn(s: &str) -> &str {
    match s {
        "unit" => "unit-test",
        "doc" => "doctest",
        val => val,
    }
}

#[test]
fn unit_test() {
    assert_eq!(generic_fn("unit", 1), Ok("unit-test"));
    assert_eq!(non_generic_fn("unit"), "unit-test");
}
