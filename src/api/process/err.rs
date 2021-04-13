pub struct Error<T> {
    pub error: anyhow::Error,
    pub value: Option<T>,
}

impl<T, E: Into<anyhow::Error>> From<E> for Error<T> {
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
            value: None,
        }
    }
}
