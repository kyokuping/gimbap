use std::fmt::Debug;

#[derive(Debug)]
pub enum GimbapError {
    ParseError(GimbapParseError),
}

#[derive(Debug)]
pub enum GimbapParseError {
    HeaderName(Box<dyn Debug>),
    HeaderValue(Box<dyn Debug>),
}

impl std::fmt::Display for GimbapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GimbapError::ParseError(parse_error) => {
                let (target, source) = match parse_error {
                    GimbapParseError::HeaderName(source) => ("header name", source),
                    GimbapParseError::HeaderValue(source) => ("header value", source),
                };
                write!(
                    f,
                    "Failed to parse the following into a {}: {:?}",
                    target, source
                )
            }
        }
    }
}
