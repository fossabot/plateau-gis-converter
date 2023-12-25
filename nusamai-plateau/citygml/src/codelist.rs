use crate::parser::ParseError;
use url::Url;

pub trait CodeResolver {
    fn resolve(
        &self,
        base_url: &Url,
        code_space: &str,
        code: &str,
    ) -> Result<Option<String>, ParseError>;
}

pub struct NoopResolver {}

impl CodeResolver for NoopResolver {
    fn resolve(
        &self,
        _base_url: &Url,
        _code_space: &str,
        _code: &str,
    ) -> Result<Option<String>, ParseError> {
        Ok(None)
    }
}
