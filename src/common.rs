use mime::Mime;
use std::{
    collections::{HashMap, hash_map::Entry},
    convert::Infallible,
    str::FromStr,
};
use tempfile::SpooledTempFile;

#[derive(Debug)]
pub enum BodyData {
    Json(serde_json::Value),
    FormData(HashMap<String, FormDataValue>),
    Text(String),
    Other {
        content_type: Option<Mime>,
        data: Vec<u8>,
    },
    Empty,
}

#[derive(Debug)]
pub enum FormDataValue {
    Text(String),
    File {
        filename: String,
        content_type: String,
        data: SpooledTempFile,
    },
}

type HeaderName = String;
type HeaderValue = Vec<String>;

#[derive(Debug, PartialEq)]
pub struct Headers(HashMap<HeaderName, HeaderValue>);

impl Default for Headers {
    fn default() -> Self {
        Self::new()
    }
}

impl Headers {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get(&self, key: &str) -> Option<&HeaderValue> {
        self.0.get(&key.to_lowercase())
    }

    pub fn append(&mut self, key: String, values: &[String]) {
        self.entry(key)
            .or_default()
            .extend(values.iter().map(|s| s.to_string()));
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(&key.to_lowercase())
    }

    pub fn insert(&mut self, key: String, value: Vec<String>) {
        self.0.insert(key.to_lowercase(), value);
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, HeaderName, HeaderValue> {
        self.0.entry(key.to_lowercase())
    }
}

pub struct StatusCode(u16);
#[derive(Debug)]
pub struct InvalidStatusCode;

impl Default for StatusCode {
    fn default() -> Self {
        StatusCode(200)
    }
}

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
