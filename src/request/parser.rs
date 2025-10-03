use derive_builder::Builder;
use mime::Mime;
use once_cell::unsync::Lazy;
use std::collections::HashMap;
use std::io::BufRead;
use std::str::FromStr;
use std::sync::LazyLock;
use url::Host;

static URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^([A-Z]+?) ([^ ]+?) (HTTP/[0-9.]+?)\s*$").unwrap());
static HEADER_NAME_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9!#\$%&'*+-.^_`|~]+$").unwrap());
static HEADER_VALUE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\\t\\u0020-\\u007E\\u0080-\\u00FF]*$").unwrap());
static AUTHORIZATION_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\\w+ .+$").unwrap());
static ACCEPT_PART_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(\\*|\\w+)/(\\*|[-.\\w+]+)(;\\s*q=\\d(\\.\\d+)?)?$").unwrap()
});

pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub version: HttpVersion,
    pub headers: HashMap<String, Vec<String>>,
    pub header_metadata: HeaderMetadata,
}
#[derive(Builder)]
pub struct HeaderMetadata {
    pub host: Host,
    pub body_metadata: Option<BodyMetadata>,
}
#[derive(Clone, Builder)]
pub struct BodyMetadata {
    pub content_type: Mime,
    pub content_length: u64,
}

pub fn parse_connection<T: BufRead>(reader: &mut T) -> Result<Request, Box<dyn std::error::Error>> {
    let mut line = String::new();
    let len = reader.read_line(&mut line)?;
    if len == 0 {
        return Err("Unexpected end of stream".into());
    }
    let (method, path, version) = parse_start_line(&line)?;
    let (headers, header_metadata) = parse_headers(reader)?;
    Ok(Request {
        method,
        path,
        version,
        headers,
        header_metadata,
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

pub fn parse_headers<T: BufRead>(
    reader: &mut T,
) -> Result<(HashMap<String, Vec<String>>, HeaderMetadata), Box<dyn std::error::Error>> {
    let mut headers = HashMap::new();
    let mut header_metadata_builder = HeaderMetadataBuilder::create_empty();
    let mut body_metadata_builder = Lazy::new(BodyMetadataBuilder::create_empty);

    for line_result in reader.lines() {
        let line = line_result?;
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

            match key.as_str() {
                "host" => {
                    if let Some(host) = values.first() {
                        header_metadata_builder.host(Host::parse(host)?);
                    }
                }
                "content-length" => {
                    if let Some(length) = values.first() {
                        body_metadata_builder.content_length(length.parse::<u64>()?);
                    }
                }
                "content-type" => {
                    if let Some(content_type) = values.first() {
                        body_metadata_builder.content_type(Mime::from_str(content_type)?);
                    }
                }
                _ => {}
            }

            headers
                .entry(key)
                .or_insert_with(Vec::new)
                .extend_from_slice(&values);
        }
    }

    let body_metadata = match Lazy::get(&body_metadata_builder) {
        Some(builder) => Some(builder.build()?),
        None => None,
    };
    header_metadata_builder.body_metadata(body_metadata);
    Ok((headers, header_metadata_builder.build()?))
}

fn validate_special_header(key: &str, value: &str) -> bool {
    match key {
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
