use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read};
use std::sync::LazyLock;

static URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^([A-Z]+?) ([^ ]+?) (HTTP/[0-9.]+?)\s*$").unwrap());
static HEADER_NAME_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9!#\$%&'*+-.^_`|~]+$").unwrap());
static HEADER_VALUE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\\t\\u0020-\\u007E\\u0080-\\u00FF]*$").unwrap());
static CONTENT_TYPE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\\w+-.]+/[-.\\w+]+.*$").unwrap());
static AUTHORIZATION_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\\w+ .+$").unwrap());
static ACCEPT_PART_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(\\*|\\w+)/(\\*|[-.\\w+]+)(;\\s*q=\\d(\\.\\d+)?)?$").unwrap()
});

pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub version: HttpVersion,
}

pub fn parse_connection<T: Read>(stream: T) -> Result<Request, Box<dyn std::error::Error>> {
    let mut line = String::new();
    let mut buf_reader = BufReader::new(stream);
    let len = buf_reader.read_line(&mut line)?;
    if len == 0 {
        return Err("Unexpected end of stream".into());
    }
    let (method, path, version) = parse_start_line(&line)?;
    Ok(Request {
        method,
        path,
        version,
    })
}

fn parse_start_line(line: &str) -> Result<(HttpMethod, String, HttpVersion), String> {
    let captures = URL_REGEX.captures(line).ok_or("Invalid request line")?;
    let method = HttpMethod::from_str(&captures[1]);
    let url = captures[2].to_string();
    let version = HttpVersion::from_str(&captures[3]);
    Ok((method, url, version))
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

impl HttpMethod {
    fn from_str(method: &str) -> Self {
        match method {
            "GET" => HttpMethod::GET,
            "HEAD" => HttpMethod::HEAD,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "CONNECT" => HttpMethod::CONNECT,
            "TRACE" => HttpMethod::TRACE,
            "PATCH" => HttpMethod::PATCH,
            _ => HttpMethod::Other(method.to_string()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum HttpVersion {
    V1_1,
}

impl HttpVersion {
    fn from_str(version: &str) -> Self {
        match version {
            "HTTP/1.1" => HttpVersion::V1_1,
            _ => panic!("Unsupported HTTP version"),
        }
    }
}

pub fn parse_headers<T: Read>(stream: T) -> Result<HashMap<String, Vec<String>>, String> {
    let mut headers = HashMap::new();

    let reader = BufReader::new(stream);

    for line_result in reader.lines() {
        let line = line_result.map_err(|e| e.to_string())?;
        if line.is_empty() {
            break;
        }

        if let Some((key_str, values_str)) = line.split_once(":") {
            let key = key_str.trim().to_lowercase();
            if !HEADER_NAME_REGEX.is_match(&key) {
                continue;
            }

            let values: Vec<String> = values_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            if values
                .iter()
                .any(|value| !HEADER_VALUE_REGEX.is_match(value))
            {
                continue;
            }

            if values
                .iter()
                .any(|value| !validate_special_header(&key, value))
            {
                continue;
            }

            headers
                .entry(key)
                .or_insert_with(Vec::new)
                .extend_from_slice(&values);
        }
    }
    Ok(headers)
}

fn validate_special_header(key: &str, value: &str) -> bool {
    match key {
        "content-type" => CONTENT_TYPE_REGEX.is_match(value),
        "authorization" => AUTHORIZATION_REGEX.is_match(value),
        "accept" => {
            if value.is_empty() {
                false
            } else {
                value
                    .split(",")
                    .all(|part| ACCEPT_PART_REGEX.is_match(part.trim()))
            }
        }
        _ => true,
    }
}

pub struct RequestMetadata {
    pub content_length: u64,
    pub content_type: String,
    pub host: String,
}
