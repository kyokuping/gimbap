use std::{convert::Infallible, str::FromStr};

pub struct StatusCode(u16);
#[derive(Debug)]
pub struct InvalidStatusCode;

impl StatusCode {
    pub fn from_u16(code: u16) -> Result<StatusCode, InvalidStatusCode> {
        if (100..=999).contains(&code) {
            Ok(StatusCode(code))
        } else {
            Err(InvalidStatusCode)
        }
    }
    pub fn as_u16(&self) -> u16 {
        self.0
    }

    pub const OK: StatusCode = StatusCode(200);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    pub const INTERNAL_SERVER_ERROR: StatusCode = StatusCode(500);
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum HttpMethod {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    TRACE,
    PATCH,
    Other(String),
}

impl FromStr for HttpMethod {
    type Err = Infallible;

    fn from_str(method: &str) -> Result<HttpMethod, Infallible> {
        Ok(match method {
            "GET" => HttpMethod::GET,
            "HEAD" => HttpMethod::HEAD,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "CONNECT" => HttpMethod::CONNECT,
            "TRACE" => HttpMethod::TRACE,
            "PATCH" => HttpMethod::PATCH,
            _ => HttpMethod::Other(method.to_string()),
        })
    }
}
