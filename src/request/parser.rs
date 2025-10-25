use derive_builder::Builder;
use flate2::read::{DeflateDecoder, GzDecoder};
use mime::Mime;
use once_cell::unsync::Lazy;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::str::FromStr;
use std::sync::LazyLock;
use url::Host;

const MAX_BODY_SIZE: u64 = 10 * 1024 * 1024; // 10MB

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

pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub version: HttpVersion,
    pub headers: HashMap<String, Vec<String>>,
    pub header_metadata: HeaderMetadata,
    pub body: Option<Body>,
}

#[derive(Builder, Debug)]
pub struct HeaderMetadata {
    pub host: Host,
    pub body_metadata: Option<BodyMetadata>,
}

#[derive(Clone, Builder, Debug)]
pub struct BodyMetadata {
    pub content_type: Mime,
    pub content_length: ContentLength,
    #[builder(default)]
    pub content_encoding: Option<Vec<ContentEncoding>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContentLength {
    Fixed(u64),
    Chunked,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum ContentEncoding {
    Gzip,
    Compress,
    Deflate,
    Br,
    Zstd,
    /// Dictionary-Compressed Brotli
    Dcb,
    /// Dictionary-Compressed Zstd
    Dcz,
}

impl FromStr for ContentEncoding {
    type Err = String;

    fn from_str(encoding: &str) -> Result<Self, Self::Err> {
        match encoding {
            "gzip" => Ok(ContentEncoding::Gzip),
            "compress" => Ok(ContentEncoding::Compress),
            "deflate" => Ok(ContentEncoding::Deflate),
            "br" => Ok(ContentEncoding::Br),
            "zstd" => Ok(ContentEncoding::Zstd),
            "dcb" => Ok(ContentEncoding::Dcb),
            "dcz" => Ok(ContentEncoding::Dcz),
            _ => Err(format!("Unknown content encoding: {}", encoding)),
        }
    }
}

pub fn parse_connection<T: BufRead + 'static>(
    mut reader: T,
) -> Result<Request, Box<dyn std::error::Error>> {
    let mut line = String::new();
    let len = reader.read_line(&mut line)?;
    if len == 0 {
        return Err("Unexpected end of stream".into());
    }
    let (method, path, version) = parse_start_line(&line)?;
    let (headers, header_metadata) = parse_headers(&mut reader)?;
    let body = match &header_metadata.body_metadata {
        Some(metadata) => Some(Body::try_new(reader, metadata.clone())?),
        None => None,
    };
    Ok(Request {
        method,
        path,
        version,
        headers,
        header_metadata,
        body,
    })
}

fn parse_start_line(line: &str) -> Result<(HttpMethod, String, HttpVersion), String> {
    let captures = URL_REGEX.captures(line).ok_or("Invalid request line")?;
    let method = HttpMethod::from_str(&captures[1]);
    let url = captures[2].to_string();
    let version = HttpVersion::from_str(&captures[3]);
    Ok((method, url, version))
}

type ParsedHeader = (HashMap<String, Vec<String>>, HeaderMetadata);

pub fn parse_headers<T: BufRead>(
    reader: &mut T,
) -> Result<ParsedHeader, Box<dyn std::error::Error>> {
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
                "content-type" => {
                    if let Some(content_type) = values.first() {
                        body_metadata_builder.content_type(Mime::from_str(content_type)?);
                    }
                }
                "content-length" => {
                    if let Some(length) = values.first() {
                        body_metadata_builder.content_length(ContentLength::Fixed(length.parse()?));
                    }
                }
                "transfer-encoding" => {
                    if let Some(encoding) = values.first() {
                        if encoding == "chunked" {
                            body_metadata_builder.content_length(ContentLength::Chunked);
                        } else {
                            return Err(format!("transfer-encoding `{encoding}` is not supported yet").into());
                        }
                    }
                }
                "content-encoding" => {
                    if !values.is_empty() {
                        body_metadata_builder.content_encoding(Some(
                            values
                                .iter()
                                .map(|v| ContentEncoding::from_str(v))
                                .collect::<Result<_, _>>()?,
                        ));
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

pub struct Body {
    pub reader: Box<dyn Read>,
    pub size_hint: Option<u64>,
    pub encoding: Option<Vec<ContentEncoding>>,
}

impl Body {
    pub fn try_new<T: Read + 'static>(
        reader: T,
        metadata: BodyMetadata,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(match metadata.content_length {
            ContentLength::Fixed(length) if length > 0 => Body {
                reader: Box::new(reader),
                encoding: metadata.content_encoding,
                size_hint: Some(length),
            },
            ContentLength::Fixed(_) => return Err("Invalid content length".into()),
            ContentLength::Chunked => Body {
                reader: Box::new(reader),
                encoding: metadata.content_encoding,
                size_hint: None,
            },
        })
    }

    pub fn into_reader(self) -> Result<Box<dyn Read + 'static>, Box<dyn std::error::Error>> {
        let decoder = match self.encoding {
            Some(encodings) => {
                let mut decoder = self.reader;
                for encoding in encodings.into_iter().rev() {
                    decoder = Body::wrap_encoding_decoder(decoder, encoding)?;
                }
                decoder
            }
            None => self.reader,
        };

        Ok(Box::new(
            decoder.take(self.size_hint.unwrap_or(MAX_BODY_SIZE)),
        ))
    }

    fn wrap_encoding_decoder(
        reader: Box<dyn Read + 'static>,
        encoding: ContentEncoding,
    ) -> Result<Box<dyn Read + 'static>, Box<dyn std::error::Error>> {
        Ok(match encoding {
            ContentEncoding::Gzip => Box::new(GzDecoder::new(reader)),
            ContentEncoding::Deflate => Box::new(DeflateDecoder::new(reader)),
            ContentEncoding::Br => Box::new(brotli::Decompressor::new(reader, 4096)),
            ContentEncoding::Zstd => Box::new(zstd::stream::Decoder::new(reader)?),
            ContentEncoding::Compress | ContentEncoding::Dcb | ContentEncoding::Dcz => {
                return Err(format!("{:?} encoding not supported yet", encoding).into());
            }
        })
    }
}
