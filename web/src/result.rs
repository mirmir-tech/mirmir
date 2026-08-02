pub fn string<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, String> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(error.to_string()),
    }
}
