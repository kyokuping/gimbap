use crate::error::{GimbapError, GimbapParseError};
use mime::Mime;
use std::{collections::HashMap, convert::Infallible, ops::Deref, str::FromStr, sync::LazyLock};
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

static HEADER_NAME_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9!#\$%&'*+-.^_`|~]+$").unwrap());
static HEADER_VALUE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\\t\\u0020-\\u007E\\u0080-\\u00FF]*$").unwrap());
static AUTHORIZATION_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\w+ .+$").unwrap());

#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct HeaderName(String);

impl Deref for HeaderName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for HeaderName {
    type Err = GimbapError;

    fn from_str(key_str: &str) -> Result<Self, Self::Err> {
        HeaderName::parse(key_str).ok_or(GimbapError::ParseError(GimbapParseError::HeaderName(
            Box::new(key_str.to_owned()),
        )))
    }
}

impl HeaderName {
    pub fn new(key_str: &str) -> Self {
        let key = key_str.trim().to_lowercase();
        Self(key)
    }

    pub fn parse(key_str: &str) -> Option<Self> {
        let key = key_str.trim().to_lowercase();
        HEADER_NAME_REGEX.is_match(&key).then_some(Self(key))
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct HeaderValue(Vec<String>);

impl Deref for HeaderValue {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// impl DerefMut for HeaderValue {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

impl IntoIterator for HeaderValue {
    type Item = String;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromStr for HeaderValue {
    type Err = GimbapError;

    fn from_str(values_str: &str) -> Result<Self, Self::Err> {
        HeaderValue::parse(values_str).ok_or(GimbapError::ParseError(
            GimbapParseError::HeaderValue(Box::new(values_str.to_owned())),
        ))
    }
}

impl TryFrom<Vec<String>> for HeaderValue {
    type Error = GimbapError;

    fn try_from(values: Vec<String>) -> Result<Self, Self::Error> {
        HeaderValue::parse_vec(values).map_err(|values| {
            GimbapError::ParseError(GimbapParseError::HeaderValue(Box::new(values)))
        })
    }
}

impl TryFrom<&str> for HeaderValue {
    type Error = GimbapError;

    fn try_from(values: &str) -> Result<Self, Self::Error> {
        HeaderValue::parse(values).ok_or(GimbapError::ParseError(GimbapParseError::HeaderValue(
            Box::new(values.to_owned()),
        )))
    }
}

impl<S: AsRef<str>> Extend<S> for HeaderValue {
    fn extend<T: IntoIterator<Item = S>>(&mut self, iter: T) {
        self.0
            .extend(iter.into_iter().map(|s| Self::normalize_one(s.as_ref())));
    }
}

impl HeaderValue {
    pub fn new(values: &[&str]) -> Self {
        Self(
            values
                .iter()
                .map(|s| HeaderValue::normalize_one(s))
                .collect(),
        )
    }

    pub fn parse(values_str: &str) -> Option<Self> {
        let values = values_str
            .split(',')
            .map(HeaderValue::normalize_one)
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();
        Self::parse_vec(values).ok()
    }

    pub fn parse_vec(values: Vec<String>) -> Result<Self, Vec<String>> {
        if values.iter().all(|v| HeaderValue::validate_one(v)) {
            Ok(Self(values))
        } else {
            Err(values)
        }
    }

    fn normalize_one(value: &str) -> String {
        value.trim().to_string()
    }

    fn validate_one(value: &str) -> bool {
        HEADER_VALUE_REGEX.is_match(value)
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct Headers(HashMap<HeaderName, HeaderValue>);

impl Deref for Headers {
    type Target = HashMap<HeaderName, HeaderValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// impl DerefMut for Headers {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

impl Headers {
    pub fn get_str(&self, key: &str) -> Option<&HeaderValue> {
        self.0.get(&HeaderName::new(key))
    }

    pub fn append<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, key: HeaderName, values: I) {
        self.0.entry(key).or_default().extend(values)
    }

    pub fn is_special_valid(key: &HeaderName, value: &str) -> bool {
        match &**key {
            "authorization" => AUTHORIZATION_REGEX.is_match(value),
            "accept" => {
                !value.is_empty()
                    && value
                        .split(",")
                        .all(|part| Mime::from_str(part.trim()).is_ok())
            }
            "content-length" => value.parse::<u64>().is_ok(),
            _ => true,
        }
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
