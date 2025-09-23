use std::io::{BufRead as _, BufReader, Read};
use std::sync::LazyLock;

static URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^([A-Z]+?) ([^ ]+?) (HTTP/[0-9.]+?)\s*$").unwrap());

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
